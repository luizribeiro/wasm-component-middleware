#![allow(missing_docs)]

use std::time::Duration;

use wasm_component_middleware::{Chain, InvocationContext, Logger, MiddlewareCtx, MiddlewareView};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::p2::pipe::MemoryOutputPipe;
use wasmtime_wasi::{HostWallClock, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../../support/fixtures/wasi-p2/wit",
    world: "workload",
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

fn main() -> wasmtime::Result<()> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, guest_build::wasi_p2())?;
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker)?;

    let stdout = MemoryOutputPipe::new(4096);
    let mut wasi = WasiCtxBuilder::new();
    wasi.env("GREETING", "Hello from WASI")
        .stdout(stdout.clone())
        .wall_clock(ReviewClock);
    let chain = Chain::builder().layer(Logger::stderr()).build();
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("wasi-p2")),
            table: ResourceTable::new(),
            wasi: wasi.build(),
        },
    );
    let guest = Workload::instantiate(&mut store, &component, &linker)?;
    guest.call_basic_system(&mut store)?;
    print!("{}", String::from_utf8(stdout.contents().to_vec())?);
    Ok(())
}
