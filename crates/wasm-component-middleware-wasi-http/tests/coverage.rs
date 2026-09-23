#![allow(missing_docs)]

use std::path::{Path, PathBuf};

use wasm_component_middleware::{
    Chain, InvocationContext, MiddlewareCtx, MiddlewareView, verify_routing,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};
use wit_component::{ComponentEncoder, StringEncoding, dummy_module, embed_component_metadata};
use wit_parser::{ManglingAndAbi, Resolve};

struct State {
    middleware: MiddlewareCtx<Self>,
    table: ResourceTable,
    wasi: WasiCtx,
    http: WasiHttpCtx,
    hooks: wasm_component_middleware_wasi_http::DefaultHooks,
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

impl WasiHttpView for State {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
            hooks: &mut self.hooks,
        }
    }
}

fn component(engine: &Engine, http_wit: &Path, wasi_wit: &Path) -> Component {
    let mut resolve = Resolve::default();
    resolve.push_dir(wasi_wit).unwrap();
    let (package, _) = resolve.push_dir(http_wit).unwrap();
    let world = resolve.select_world(&[package], Some("imports")).unwrap();
    let mut module = dummy_module(&resolve, world, ManglingAndAbi::Standard32);
    embed_component_metadata(&mut module, &resolve, world, StringEncoding::UTF8).unwrap();
    let component = ComponentEncoder::default()
        .module(&module)
        .unwrap()
        .validate(true)
        .encode()
        .unwrap();
    Component::new(engine, component).unwrap()
}

fn state() -> State {
    let chain = Chain::builder().build();
    State {
        middleware: MiddlewareCtx::new(chain, InvocationContext::new("coverage")),
        table: ResourceTable::new(),
        wasi: WasiCtxBuilder::new().build(),
        http: WasiHttpCtx::new(),
        hooks: wasm_component_middleware_wasi_http::DefaultHooks,
    }
}

#[test]
fn all_preview_2_http_imports_are_routed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine = Engine::default();
    let component = component(
        &engine,
        &root.join("wit"),
        &root.join("../wasm-component-middleware-wasi/wit"),
    );
    let mut linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker).unwrap();
    wasm_component_middleware_wasi_http::p2::add_only_http_to_linker_sync(&mut linker).unwrap();

    verify_routing(
        &engine,
        &component,
        wasm_component_middleware_wasi::p2::ROUTED_INTERFACES
            .iter()
            .chain(wasm_component_middleware_wasi_http::p2::ROUTED_INTERFACES)
            .copied(),
        [],
    )
    .unwrap();
    drop(state());
}

#[test]
fn all_preview_3_http_imports_are_routed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config).unwrap();
    let component = component(
        &engine,
        &root.join("wit-p3"),
        &root.join("../wasm-component-middleware-wasi/wit-p3"),
    );
    let mut linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi_http::p3::add_to_linker(&mut linker).unwrap();

    verify_routing(
        &engine,
        &component,
        wasm_component_middleware_wasi_http::p3::ROUTED_INTERFACES
            .iter()
            .copied(),
        [],
    )
    .unwrap();
}
