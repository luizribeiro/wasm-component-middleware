#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::path::PathBuf;
use wasm_component_middleware::{
    Chain, InvocationContext, MiddlewareCtx, MiddlewareView, verify_routing,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine};
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
    const ALLOWED_MISSING: &[&str] = &[
        "wasi:filesystem/preopens",
        "wasi:filesystem/types",
        "wasi:random/insecure",
        "wasi:random/insecure-seed",
        "wasi:random/random",
        "wasi:sockets/instance-network",
        "wasi:sockets/ip-name-lookup",
        "wasi:sockets/network",
        "wasi:sockets/tcp",
        "wasi:sockets/tcp-create-socket",
        "wasi:sockets/udp",
        "wasi:sockets/udp-create-socket",
    ];

    let engine = Engine::default();
    let component = all_imports_component(&engine);
    let mut linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker).unwrap();
    let error = verify_routing(
        &engine,
        &component,
        wasm_component_middleware_wasi::p2::ROUTED_INTERFACES
            .iter()
            .copied(),
        [],
    )
    .unwrap_err();

    let actual = error
        .functions()
        .iter()
        .map(|function| function.split_once('@').unwrap().0)
        .collect::<BTreeSet<_>>();
    let expected = ALLOWED_MISSING.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(actual, expected, "{error}");
}

#[test]
fn only_ungated_p3_interfaces_are_unrouted() {
    const ALLOWED_MISSING: &[&str] = &[
        "wasi:filesystem/preopens",
        "wasi:filesystem/types",
        "wasi:random/insecure",
        "wasi:random/insecure-seed",
        "wasi:random/random",
        "wasi:sockets/ip-name-lookup",
        "wasi:sockets/types",
    ];

    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config).unwrap();
    let component = all_p3_imports_component(&engine);
    let mut linker = Linker::<State>::new(&engine);
    wasm_component_middleware_wasi::p3::add_to_linker(&mut linker).unwrap();
    let error = verify_routing(
        &engine,
        &component,
        wasm_component_middleware_wasi::p3::ROUTED_INTERFACES
            .iter()
            .copied(),
        [],
    )
    .unwrap_err();

    let actual = error
        .functions()
        .iter()
        .map(|function| function.split_once('@').unwrap().0)
        .collect::<BTreeSet<_>>();
    let expected = ALLOWED_MISSING.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(actual, expected, "{error}");
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

#[allow(dead_code)]
fn state() -> State {
    State {
        middleware: Some(MiddlewareCtx::new(
            Chain::builder().build(),
            InvocationContext::new("coverage"),
        )),
        table: ResourceTable::new(),
        wasi: WasiCtxBuilder::new().build(),
    }
}
