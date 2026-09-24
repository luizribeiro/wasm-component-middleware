#![allow(missing_docs)]

use std::fs;
use std::sync::{Arc, Mutex};

use wasm_component_middleware::{
    ArgumentValue, Budget, Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx,
    MiddlewareView, Outcome,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{FsPerms, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../../guests/wasi-p3/wit",
    world: "workload",
    imports: { default: async | store },
    exports: { default: async | store },
    with: { "wasi": wasmtime_wasi::p3::bindings },
    require_store_data_send: true,
});

struct State {
    middleware: MiddlewareCtx<Self>,
    table: ResourceTable,
    wasi: WasiCtx,
}

impl MiddlewareView for State {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        &mut self.middleware
    }
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

#[derive(Default)]
struct Totals {
    bytes: usize,
    chunks: usize,
}

struct ChunkLogger(Arc<Mutex<Totals>>);

impl Layer<State> for ChunkLogger {
    type Frame = bool;

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<bool, Denied> {
        let chunk = call.function == "[stream-read]read-via-stream";
        if chunk {
            let mut totals = self.0.lock().unwrap();
            totals.bytes += call
                .args
                .get("bytes")
                .and_then(ArgumentValue::byte_len)
                .unwrap_or_default();
            totals.chunks += 1;
        }
        Ok(chunk)
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, chunk: bool, outcome: Outcome<'_>) {
        if chunk && matches!(outcome, Outcome::Failed(_)) {
            let totals = self.0.lock().unwrap();
            eprintln!(
                "stream read: {} bytes in {} chunks, denied",
                totals.bytes, totals.chunks
            );
        }
    }
}

#[tokio::main]
async fn main() -> wasmtime::Result<()> {
    let directory = tempfile::tempdir()?;
    fs::write(
        directory.path().join("first.bin"),
        generated_bytes(64 * 1024),
    )?;
    fs::write(
        directory.path().join("input.bin"),
        generated_bytes(1024 * 1024),
    )?;

    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, test_guests::wasi_p3())?;
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker)?;
    wasm_component_middleware_wasi::p3::add_to_linker_with_stream_relay(
        &mut linker,
        wasm_component_middleware_wasi::p3::StreamRelay::default(),
    )?;

    let totals = Arc::new(Mutex::new(Totals::default()));
    let chain = Chain::builder()
        .layer(ChunkLogger(Arc::clone(&totals)))
        .layer(Budget::new(100 * 1024, |call: &Call<'_>| {
            (call.function == "[stream-read]read-via-stream")
                .then(|| {
                    call.args
                        .get("bytes")
                        .and_then(ArgumentValue::byte_len)
                        .map(|bytes| ("filesystem", u64::try_from(bytes).unwrap_or(u64::MAX)))
                })
                .flatten()
        }))
        .build();
    let first = read_file(
        &engine,
        &component,
        &linker,
        Arc::clone(&chain),
        directory.path(),
        "first.bin",
    )
    .await?;
    let second = read_file(
        &engine,
        &component,
        &linker,
        chain,
        directory.path(),
        "input.bin",
    )
    .await?;
    println!("first guest: {first}");
    println!("second guest: {second}");
    Ok(())
}

async fn read_file(
    engine: &Engine,
    component: &Component,
    linker: &Linker<State>,
    chain: Arc<Chain<State>>,
    directory: &std::path::Path,
    path: &str,
) -> wasmtime::Result<String> {
    let mut wasi = WasiCtxBuilder::new();
    wasi.preopened_dir(directory, ".", FsPerms::ReadOnly)?;
    let mut store = Store::new(
        engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("byte-budget")),
            table: ResourceTable::new(),
            wasi: wasi.build(),
        },
    );
    let guest = Workload::instantiate_async(&mut store, component, linker).await?;
    let result = store
        .run_concurrent(async |accessor| {
            guest
                .call_stream_read(accessor, path.to_owned(), false)
                .await
        })
        .await??;
    Ok(result)
}

fn generated_bytes(size: usize) -> Vec<u8> {
    (0..size)
        .map(|index| u8::try_from((index * 31 + index / 7) & 0xff).unwrap_or_default())
        .collect()
}
