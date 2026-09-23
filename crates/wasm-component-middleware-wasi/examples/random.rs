#![allow(missing_docs)]

use wasm_component_middleware::{
    Call, Chain, Denied, InvocationContext, Layer, Logger, MiddlewareCtx, MiddlewareView, Outcome,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::random::Deterministic;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../../guests/wasi-p2/wit",
    world: "workload",
});

struct State {
    middleware: MiddlewareCtx<Self>,
    table: ResourceTable,
    wasi: WasiCtx,
    random_calls: usize,
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

struct RandomCallLimit(usize);

impl Layer<State> for RandomCallLimit {
    type Frame = ();

    fn before(&self, state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        if call
            .interface
            .is_some_and(|interface| interface.starts_with("wasi:random/"))
        {
            if state.random_calls >= self.0 {
                return Err(Denied::new(format!(
                    "random call limit of {} reached",
                    self.0
                )));
            }
            state.random_calls += 1;
        }
        Ok(())
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

fn main() -> wasmtime::Result<()> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, test_guests::wasi_p2())?;
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker)?;

    let stdout = MemoryOutputPipe::new(4096);
    let mut wasi = WasiCtxBuilder::new();
    wasi.stdout(stdout.clone())
        .secure_random(Deterministic::new(vec![0, 0, 0, 1]));
    let chain = Chain::builder()
        .layer(Logger::stderr())
        .layer(RandomCallLimit(3))
        .build();
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("random")),
            table: ResourceTable::new(),
            wasi: wasi.build(),
            random_calls: 0,
        },
    );
    let guest = Workload::instantiate(&mut store, &component, &linker)?;
    let error = guest.call_random_demo(&mut store).unwrap_err();

    print!("{}", String::from_utf8(stdout.contents().to_vec())?);
    let denial = error
        .downcast_ref::<Denied>()
        .ok_or_else(|| wasmtime::Error::msg("guest trapped without a middleware denial"))?;
    eprintln!("guest trapped: {}", denial.reason());
    Ok(())
}
