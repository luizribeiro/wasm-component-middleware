#![allow(missing_docs)]

use std::collections::HashSet;
use std::process::Command;
use std::sync::{Arc, Mutex};

use wasm_component_middleware::{
    Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx, MiddlewareView, Outcome, Routed,
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

#[derive(Clone, Copy)]
enum UserName {
    Ada,
    Fail,
}

struct State {
    middleware: MiddlewareCtx<Self>,
    table: ResourceTable,
    wasi: WasiCtx,
    user_name: UserName,
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
        match self.user_name {
            UserName::Ada => Ok("Ada".to_owned()),
            UserName::Fail => Err(wasmtime::Error::msg("identity backend failed")),
        }
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

#[derive(Default)]
struct Observations {
    ids: Vec<u64>,
    failed_user_name: bool,
}

struct Observer(Arc<Mutex<Observations>>);

impl Layer<State> for Observer {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        self.0.lock().unwrap().ids.push(call.id);
        Ok(())
    }

    fn after(&self, _state: &mut State, call: &Call<'_>, (): (), outcome: Outcome<'_>) {
        if call.function == "user-name" && matches!(outcome, Outcome::Failed(_)) {
            self.0.lock().unwrap().failed_user_name = true;
        }
    }
}

struct DenyUserName;

impl Layer<State> for DenyUserName {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        if call.function == "user-name" {
            Err(Denied::new(format!("{} is denied", call.function)))
        } else {
            Ok(())
        }
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

fn invoke(chain: Chain<State>, user_name: UserName) -> wasmtime::Result<String> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, test_guests::hello())?;
    let mut linker = Linker::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    example::hello::host::add_to_linker::<_, Routed<State>>(&mut linker, Routed::<State>::get)?;
    verify_routing(&engine, &component, [HELLO_HOST], ["wasi:"])?;
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(Arc::new(chain), InvocationContext::new("hello")),
            table: ResourceTable::new(),
            wasi: WasiCtxBuilder::new().build(),
            user_name,
        },
    );
    let hello = Hello::instantiate(&mut store, &component, &linker)?;

    route_export(store.as_context_mut(), None, "greet", &"Hello", |store| {
        hello.call_greet(store, "Hello")
    })
}

#[test]
fn hello_imports_are_routed() {
    let engine = Engine::default();
    let component = Component::from_file(&engine, test_guests::hello()).unwrap();
    let mut linker = Linker::<State>::new(&engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker).unwrap();
    example::hello::host::add_to_linker::<_, Routed<State>>(&mut linker, Routed::<State>::get)
        .unwrap();

    verify_routing(&engine, &component, [HELLO_HOST], ["wasi:"]).unwrap();
}

#[test]
fn unrouted_import_is_named_before_instantiation() {
    let engine = Engine::default();
    let component = Component::from_file(&engine, test_guests::unrouted_import()).unwrap();
    let error = verify_routing(&engine, &component, [HELLO_HOST], ["wasi:"]).unwrap_err();

    assert_eq!(
        error.to_string(),
        concat!(
            "imports example:unrouted/host.secret, example:unrouted/host.classified ",
            "are not routed through the middleware chain"
        )
    );
    assert_eq!(
        error.functions(),
        [
            "example:unrouted/host.secret",
            "example:unrouted/host.classified"
        ]
    );
}

#[test]
fn trace_example_prints_nested_calls_and_greeting() {
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "-p",
            "wasm-component-middleware",
            "--example",
            "trace",
            "--locked",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "Hello, Ada!\n");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "→ #1 export greet(\"Hello\")\n  → #2 import example:hello/host.user-name()\n  ← #2 returned\n  → #3 import example:hello/host.log(\"greeting Ada\")\n  ← #3 returned\n← #1 returned\n"
    );
}

#[test]
fn denial_from_user_name_survives_the_component_boundary() {
    let observations = Arc::new(Mutex::new(Observations::default()));
    let chain = Chain::builder()
        .layer(Observer(Arc::clone(&observations)))
        .layer(DenyUserName)
        .build();

    let error = invoke(chain, UserName::Ada).unwrap_err();
    let denied = error.downcast_ref::<Denied>().unwrap();

    assert_eq!(denied.reason(), "user-name is denied");
}

#[test]
fn handler_failure_reaches_layers() {
    let observations = Arc::new(Mutex::new(Observations::default()));
    let chain = Chain::builder()
        .layer(Observer(Arc::clone(&observations)))
        .build();

    let result = invoke(chain, UserName::Fail);

    assert!(result.is_err());
    assert!(observations.lock().unwrap().failed_user_name);
}

#[test]
fn call_ids_are_unique_within_an_invocation() {
    let observations = Arc::new(Mutex::new(Observations::default()));
    let chain = Chain::builder()
        .layer(Observer(Arc::clone(&observations)))
        .build();

    assert_eq!(invoke(chain, UserName::Ada).unwrap(), "Hello, Ada!");
    let ids = &observations.lock().unwrap().ids;
    let unique = ids.iter().copied().collect::<HashSet<_>>();

    assert_eq!(ids.len(), 3);
    assert_eq!(unique.len(), ids.len());
}
