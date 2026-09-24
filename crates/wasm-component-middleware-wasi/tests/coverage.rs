#![allow(missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_component_middleware::{
    Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx, MiddlewareView, Outcome,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wit_component::{ComponentEncoder, StringEncoding, dummy_module, embed_component_metadata};
use wit_parser::{ManglingAndAbi, Resolve};

struct State {
    middleware: Option<MiddlewareCtx<Self>>,
    table: ResourceTable,
    wasi: WasiCtx,
}

impl MiddlewareView for State {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        self.middleware.as_mut().unwrap()
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

fn all_imports_component(engine: &Engine) -> Component {
    p2_imports_component(engine, "bindings")
}

fn proxy_imports_component(engine: &Engine) -> Component {
    p2_imports_component(engine, "proxy-interfaces")
}

fn p2_imports_component(engine: &Engine, world: &str) -> Component {
    let mut resolve = Resolve::default();
    let wit = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wit");
    let (package, _) = resolve.push_dir(wit).unwrap();
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
    let chain = Chain::builder().build();
    let mut store = Store::new(engine, state(&chain));
    linker.instantiate(&mut store, component).unwrap();
}

async fn instantiate_async(
    component: &Component,
    engine: &Engine,
    add: impl FnOnce(&mut Linker<State>) -> wasmtime::Result<()>,
) {
    let mut linker = Linker::new(engine);
    add(&mut linker).unwrap();
    let chain = Chain::builder().build();
    let mut store = Store::new(engine, state(&chain));
    linker
        .instantiate_async(&mut store, component)
        .await
        .unwrap();
}

fn all_p3_imports_component(engine: &Engine) -> Component {
    let mut resolve = Resolve::default();
    let wit = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wit-p3");
    resolve.push_dir(wit).unwrap();
    let package = resolve
        .packages
        .iter()
        .find(|(_, package)| {
            package.name.namespace == "wasi"
                && package.name.name == "cli"
                && package.name.version.as_ref().is_some_and(|version| {
                    version.major == 0 && version.minor == 3 && version.patch == 0
                })
        })
        .map(|(id, _)| id)
        .unwrap();
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

#[test]
fn preview_2_markers_cover_declared_interfaces() {
    let engine = Engine::default();
    let component = all_imports_component(&engine);
    wasm_component_middleware_wasi::verify_routing(&engine, &component, []).unwrap();
}

#[test]
fn preview_3_markers_cover_declared_interfaces() {
    let engine = async_engine();
    let component = all_p3_imports_component(&engine);
    wasm_component_middleware_wasi::verify_routing(&engine, &component, []).unwrap();
}

#[test]
fn verification_reports_a_deliberately_unrouted_interface() {
    let engine = Engine::default();
    let component = Component::from_file(&engine, guest_build::unrouted_import()).unwrap();

    let error =
        wasm_component_middleware_wasi::verify_routing(&engine, &component, []).unwrap_err();

    assert!(
        error
            .functions()
            .iter()
            .any(|function| function.starts_with("example:unrouted/host")),
        "{error}"
    );
}

#[test]
fn sync_preview_2_linker_defines_every_import() {
    let engine = Engine::default();
    let component = all_imports_component(&engine);
    instantiate_sync(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p2::add_to_linker_sync(linker)
    });
}

#[test]
fn sync_preview_2_options_linker_defines_every_import() {
    let engine = Engine::default();
    let component = all_imports_component(&engine);
    let options = wasmtime_wasi::p2::bindings::sync::LinkOptions::default();
    instantiate_sync(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p2::add_to_linker_with_options_sync(linker, &options)
    });
}

#[test]
fn sync_preview_2_proxy_linker_defines_every_import() {
    let engine = Engine::default();
    let component = proxy_imports_component(&engine);
    instantiate_sync(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p2::add_to_linker_proxy_interfaces_sync(linker)
    });
}

#[tokio::test]
async fn async_preview_2_linker_defines_every_import() {
    let engine = async_engine();
    let component = all_imports_component(&engine);
    instantiate_async(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p2::add_to_linker_async(linker)
    })
    .await;
}

#[tokio::test]
async fn async_preview_2_options_linker_defines_every_import() {
    let engine = async_engine();
    let component = all_imports_component(&engine);
    let options = wasmtime_wasi::p2::bindings::LinkOptions::default();
    instantiate_async(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p2::add_to_linker_with_options_async(linker, &options)
    })
    .await;
}

#[tokio::test]
async fn async_preview_2_proxy_linker_defines_every_import() {
    let engine = async_engine();
    let component = proxy_imports_component(&engine);
    instantiate_async(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p2::add_to_linker_proxy_interfaces_async(linker)
    })
    .await;
}

#[tokio::test]
async fn preview_3_linker_defines_every_import() {
    let engine = async_engine();
    let component = all_p3_imports_component(&engine);
    instantiate_async(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p3::add_to_linker(linker)
    })
    .await;
}

#[tokio::test]
async fn preview_3_options_linker_defines_every_import() {
    let engine = async_engine();
    let component = all_p3_imports_component(&engine);
    let options = wasmtime_wasi::p3::bindings::LinkOptions::default();
    instantiate_async(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p3::add_to_linker_with_options(linker, &options)
    })
    .await;
}

#[tokio::test]
async fn preview_3_stream_relay_linker_defines_every_import() {
    let engine = async_engine();
    let component = all_p3_imports_component(&engine);
    instantiate_async(&component, &engine, |linker| {
        wasm_component_middleware_wasi::p3::add_to_linker_with_stream_relay(
            linker,
            wasm_component_middleware_wasi::p3::StreamRelay::default(),
        )
    })
    .await;
}

#[test]
fn linker_still_rejects_duplicate_definitions() {
    let engine = Engine::default();
    let mut linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker).unwrap();

    let error = wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker).unwrap_err();

    assert!(error.to_string().contains("defined twice"), "{error}");
}

#[test]
fn network_error_conversion_requires_the_option_and_reaches_the_chain() {
    let engine = Engine::default();
    let component = Component::from_file(&engine, guest_build::network_error_code()).unwrap();
    let mut default_linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut default_linker).unwrap();
    let chain = Chain::builder().build();
    let mut store = Store::new(&engine, state(&chain));

    let error = default_linker
        .instantiate(&mut store, &component)
        .unwrap_err();

    assert!(
        error.to_string().contains("wasi:sockets/network"),
        "{error}"
    );

    let mut options = wasmtime_wasi::p2::bindings::sync::LinkOptions::default();
    options.network_error_code(true);
    let chain = Chain::builder().build();
    let mut plain_linker = Linker::<State>::new(&engine);
    wasmtime_wasi::p2::add_to_linker_with_options_sync(&mut plain_linker, &options).unwrap();
    let mut plain_store = Store::new(&engine, state(&chain));
    let plain_instance = plain_linker
        .instantiate(&mut plain_store, &component)
        .unwrap();
    let plain_probe = plain_instance
        .get_typed_func::<(), (bool,)>(&mut plain_store, "probe")
        .unwrap();
    let plain = plain_probe.call(&mut plain_store, ()).unwrap();

    let mut gated_linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_with_options_sync(
        &mut gated_linker,
        &options,
    )
    .unwrap();
    let mut gated_store = Store::new(&engine, state(&chain));
    let gated_instance = gated_linker
        .instantiate(&mut gated_store, &component)
        .unwrap();
    let gated_probe = gated_instance
        .get_typed_func::<(), (bool,)>(&mut gated_store, "probe")
        .unwrap();
    let gated = gated_probe.call(&mut gated_store, ()).unwrap();

    assert_eq!(gated, plain);

    let reached = Arc::new(AtomicBool::new(false));
    let chain = Chain::builder()
        .layer(ObserveNetworkErrorCode(Arc::clone(&reached)))
        .build();
    let mut linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_with_options_sync(&mut linker, &options)
        .unwrap();
    let mut store = Store::new(&engine, state(&chain));
    let instance = linker.instantiate(&mut store, &component).unwrap();
    let probe = instance
        .get_typed_func::<(), (bool,)>(&mut store, "probe")
        .unwrap();

    let (converted,) = probe.call(&mut store, ()).unwrap();

    assert!(converted);
    assert!(reached.load(Ordering::Relaxed));
}

#[test]
fn p3_linker_still_rejects_duplicate_definitions() {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config).unwrap();
    let mut linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p3::add_to_linker(&mut linker).unwrap();

    let error = wasm_component_middleware_wasi::p3::add_to_linker(&mut linker).unwrap_err();

    assert!(error.to_string().contains("defined twice"), "{error}");
}

#[derive(Clone)]
struct ObserveNetworkErrorCode(Arc<AtomicBool>);

impl Layer<State> for ObserveNetworkErrorCode {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        match call.function {
            "[method]output-stream.blocking-write-and-flush" => {
                Err(Denied::new("create a stream error"))
            }
            "network-error-code" => {
                self.0.store(true, Ordering::Relaxed);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

fn state(chain: &Arc<Chain<State>>) -> State {
    State {
        middleware: Some(MiddlewareCtx::new(
            Arc::clone(chain),
            InvocationContext::new("coverage"),
        )),
        table: ResourceTable::new(),
        wasi: WasiCtxBuilder::new().build(),
    }
}
