#![allow(missing_docs)]

use std::time::Duration;

use wasm_component_middleware::{Chain, InvocationContext, Logger, MiddlewareCtx, MiddlewareView};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{HostWallClock, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

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

struct ReviewClock;

impl HostWallClock for ReviewClock {
    fn resolution(&self) -> Duration {
        Duration::from_nanos(1)
    }

    fn now(&self) -> Duration {
        Duration::new(1_700_000_000, 123_456_789)
    }
}

#[tokio::main]
async fn main() -> wasmtime::Result<()> {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, test_guests::wasi_p3())?;
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    wasm_component_middleware_wasi::p3::add_to_linker(&mut linker)?;

    let stdout = MemoryOutputPipe::new(4096);
    let mut wasi = WasiCtxBuilder::new();
    wasi.env("GREETING", "Hello from WASI")
        .stdout(stdout.clone())
        .wall_clock(ReviewClock);
    let chain = Chain::builder().layer(Logger::stderr()).build();
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("wasi-p3")),
            table: ResourceTable::new(),
            wasi: wasi.build(),
        },
    );
    let guest = Workload::instantiate_async(&mut store, &component, &linker).await?;
    store
        .run_concurrent(async |accessor| guest.call_basic_system(accessor).await)
        .await??;
    print!("{}", String::from_utf8(stdout.contents().to_vec())?);
    Ok(())
}
