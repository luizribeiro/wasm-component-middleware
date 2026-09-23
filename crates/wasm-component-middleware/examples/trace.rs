#![allow(missing_docs)]

use std::sync::Arc;

use wasm_component_middleware::{
    Chain, InvocationContext, Logger, MiddlewareCtx, MiddlewareView, Routed, route_export,
    route_imports,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{AsContextMut, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../../guests/hello/wit",
    world: "hello",
    imports: { default: trappable },
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

impl example::hello::host::Host for State {
    fn user_name(&mut self) -> wasmtime::Result<String> {
        Ok("Ada".to_owned())
    }

    fn log(&mut self, _message: String) -> wasmtime::Result<()> {
        Ok(())
    }
}

route_imports! {
    example::hello::host::Host => State as "example:hello/host" {
        fn user_name(&mut self) -> wasmtime::Result<String>;
        fn log(&mut self, message: String) -> wasmtime::Result<()>;
    }
}

fn main() -> wasmtime::Result<()> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, test_guests::hello())?;
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    example::hello::host::add_to_linker::<_, Routed<State>>(&mut linker, Routed::<State>::get)?;

    let chain = Arc::new(Chain::builder().layer(Logger::stderr()).build());
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("hello")),
            table: ResourceTable::new(),
            wasi: WasiCtxBuilder::new().build(),
        },
    );
    let hello = Hello::instantiate(&mut store, &component, &linker)?;
    let greeting = route_export(store.as_context_mut(), None, "greet", &"Hello", |store| {
        hello.call_greet(store, "Hello")
    })?;

    println!("{greeting}");
    Ok(())
}
