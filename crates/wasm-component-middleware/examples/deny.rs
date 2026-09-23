#![allow(missing_docs)]

use std::sync::Arc;

use wasm_component_middleware::{
    Allowlist, Chain, Denied, InvocationContext, Logger, MiddlewareCtx, MiddlewareView, Routed,
    route_export, route_imports, verify_routing,
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
    const HELLO_HOST: example::hello::host::Host => State as "example:hello/host" {
        fn user_name(&mut self) -> wasmtime::Result<String>;
        fn log(&mut self, message: String) -> wasmtime::Result<()>;
    }
}

fn greet(
    engine: &Engine,
    component: &Component,
    linker: &Linker<State>,
    allowlist: Allowlist,
) -> wasmtime::Result<String> {
    let chain = Arc::new(
        Chain::builder()
            .layer(Logger::stderr())
            .layer(allowlist)
            .build(),
    );
    let mut store = Store::new(
        engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("hello")),
            table: ResourceTable::new(),
            wasi: WasiCtxBuilder::new().build(),
        },
    );
    let hello = Hello::instantiate(&mut store, component, linker)?;
    route_export(store.as_context_mut(), None, "greet", &"Hello", |store| {
        hello.call_greet(store, "Hello")
    })
}

fn main() -> wasmtime::Result<()> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, test_guests::hello())?;
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    example::hello::host::add_to_linker::<_, Routed<State>>(&mut linker, Routed::<State>::get)?;
    verify_routing(&engine, &component, [HELLO_HOST], ["wasi:"])?;

    eprintln!("denied:");
    let log_only = Allowlist::new()
        .allow_function(None, "greet")
        .allow_function(Some(HELLO_HOST.name()), "log");
    if let Err(error) = greet(&engine, &component, &linker, log_only) {
        let reason = error
            .downcast_ref::<Denied>()
            .map_or_else(|| error.to_string(), |denied| denied.reason().to_owned());
        eprintln!("greet failed: {reason}");
    }

    eprintln!("allowed:");
    let everything = Allowlist::new()
        .allow_function(None, "greet")
        .allow_interface(HELLO_HOST.name());
    let greeting = greet(&engine, &component, &linker, everything)?;
    eprintln!("{greeting}");
    Ok(())
}
