#![allow(missing_docs)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use wasm_component_middleware::{
    Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx, MiddlewareView, Outcome,
    verify_routing,
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
    let mut resolve = Resolve::default();
    let wit = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wit");
    let (package, _) = resolve.push_dir(wit).unwrap();
    let world = resolve.select_world(&[package], Some("bindings")).unwrap();
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
fn only_later_interfaces_are_unrouted() {
    let engine = Engine::default();
    let component = all_imports_component(&engine);
    let mut linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker).unwrap();
    verify_routing(
        &engine,
        &component,
        wasm_component_middleware_wasi::p2::ROUTED_INTERFACES
            .iter()
            .copied(),
        [],
    )
    .unwrap();
}

#[test]
fn only_ungated_p3_interfaces_are_unrouted() {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config).unwrap();
    let component = all_p3_imports_component(&engine);
    let mut linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p3::add_to_linker(&mut linker).unwrap();
    verify_routing(
        &engine,
        &component,
        wasm_component_middleware_wasi::p3::ROUTED_INTERFACES
            .iter()
            .copied(),
        [],
    )
    .unwrap();
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
    let component = Component::from_file(&engine, test_guests::network_error_code()).unwrap();
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

    let reached = Arc::new(AtomicBool::new(false));
    let chain = Chain::builder()
        .layer(ObserveNetworkErrorCode(Arc::clone(&reached)))
        .build();
    let mut linker = Linker::<State>::new(&engine);
    let mut options = wasmtime_wasi::p2::bindings::sync::LinkOptions::default();
    options.network_error_code(true);
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
