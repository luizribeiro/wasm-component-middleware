#![allow(missing_docs)]

use std::path::{Path, PathBuf};

use wasm_component_middleware::{
    Chain, InvocationContext, MiddlewareCtx, MiddlewareView, verify_routing,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
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

fn component(engine: &Engine, http_wit: &Path, wasi_wit: &Path, world: &str) -> Component {
    let mut resolve = Resolve::default();
    resolve.push_dir(wasi_wit).unwrap();
    let (package, _) = resolve.push_dir(http_wit).unwrap();
    let world = resolve.select_world(&[package], Some(world)).unwrap();
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

fn async_engine() -> Engine {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    Engine::new(&config).unwrap()
}

fn instantiate_sync(
    component: &Component,
    engine: &Engine,
    add: impl FnOnce(&mut Linker<State>) -> wasmtime::Result<()>,
) {
    let mut linker = Linker::new(engine);
    add(&mut linker).unwrap();
    let mut store = Store::new(engine, state());
    linker.instantiate(&mut store, component).unwrap();
}

async fn instantiate_async(
    component: &Component,
    engine: &Engine,
    add: impl FnOnce(&mut Linker<State>) -> wasmtime::Result<()>,
) {
    let mut linker = Linker::new(engine);
    add(&mut linker).unwrap();
    let mut store = Store::new(engine, state());
    linker
        .instantiate_async(&mut store, component)
        .await
        .unwrap();
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
fn preview_2_markers_cover_declared_http_interfaces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine = Engine::default();
    let component = component(
        &engine,
        &root.join("wit"),
        &root.join("../wasm-component-middleware-wasi/wit"),
        "imports",
    );
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
}

#[test]
fn preview_3_markers_cover_declared_http_interfaces() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine = async_engine();
    let component = component(
        &engine,
        &root.join("wit-p3"),
        &root.join("../wasm-component-middleware-wasi/wit-p3"),
        "imports",
    );
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

#[test]
fn sync_preview_2_http_only_linker_defines_every_import() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine = Engine::default();
    let component = component(
        &engine,
        &root.join("wit"),
        &root.join("../wasm-component-middleware-wasi/wit"),
        "imports",
    );
    instantiate_sync(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p2::add_to_linker_proxy_interfaces_sync(linker)?;
        wasm_component_middleware_wasi_http::p2::add_only_http_to_linker_sync(linker)
    });
}

#[tokio::test]
async fn async_preview_2_http_only_linker_defines_every_import() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine = async_engine();
    let component = component(
        &engine,
        &root.join("wit"),
        &root.join("../wasm-component-middleware-wasi/wit"),
        "imports",
    );
    instantiate_async(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p2::add_to_linker_proxy_interfaces_async(linker)?;
        wasm_component_middleware_wasi_http::p2::add_only_http_to_linker_async(linker)
    })
    .await;
}

#[test]
fn sync_preview_2_http_linker_defines_every_import() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine = Engine::default();
    let component = component(
        &engine,
        &root.join("wit"),
        &root.join("../wasm-component-middleware-wasi/wit"),
        "command-imports",
    );
    instantiate_sync(&component, &engine, |linker| {
        wasm_component_middleware_wasi_http::p2::add_to_linker_sync(linker)
    });
}

#[tokio::test]
async fn async_preview_2_http_linker_defines_every_import() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine = async_engine();
    let component = component(
        &engine,
        &root.join("wit"),
        &root.join("../wasm-component-middleware-wasi/wit"),
        "proxy-imports",
    );
    instantiate_async(&component, &engine, |linker| {
        wasm_component_middleware_wasi_http::p2::add_to_linker_async(linker)
    })
    .await;
}

#[tokio::test]
async fn preview_3_http_linker_defines_every_import() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let engine = async_engine();
    let component = component(
        &engine,
        &root.join("wit-p3"),
        &root.join("../wasm-component-middleware-wasi/wit-p3"),
        "imports",
    );
    instantiate_async(&component, &engine, |linker| {
        wasm_component_middleware_wasi_http::p3::add_to_linker(linker)
    })
    .await;
}
