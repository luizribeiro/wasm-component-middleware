#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::convert::Infallible;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use wasm_component_middleware::{
    ArgumentValue, Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx, MiddlewareView,
    Outcome,
};
use wasm_component_middleware_wasi_http::{DefaultHooks, WasiHttpHooks};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::io::TokioIo;
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};
use wit_parser::{Resolve, TypeDefKind};

mod p2 {
    wasmtime::component::bindgen!({
        path: "../../guests/http-p2/wit",
        world: "client",
        exports: { default: async },
        require_store_data_send: true,
    });
}

mod p3 {
    wasmtime::component::bindgen!({
        path: "../../guests/http-p3/wit",
        world: "client",
        exports: { default: async | store },
        require_store_data_send: true,
    });
}

struct State<H> {
    middleware: MiddlewareCtx<Self>,
    table: ResourceTable,
    wasi: WasiCtx,
    http: WasiHttpCtx,
    hooks: H,
}

impl<H: Send> MiddlewareView for State<H> {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        &mut self.middleware
    }
}

impl<H: Send> WasiView for State<H> {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

impl<H: wasmtime_wasi_http::WasiHttpHooks + 'static> WasiHttpView for State<H> {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
            hooks: &mut self.hooks,
        }
    }
}

#[derive(Clone, Debug)]
struct CallRecord {
    interface: String,
    function: String,
    handles: Vec<u32>,
    produced: Vec<u32>,
}

#[derive(Clone, Default)]
struct Record(Arc<Mutex<Vec<CallRecord>>>);

impl<H: Send + 'static> Layer<State<H>> for Record {
    type Frame = Option<usize>;

    fn before(&self, _: &mut State<H>, call: &Call<'_>) -> Result<Option<usize>, Denied> {
        let Some(interface) = call.interface else {
            return Ok(None);
        };
        let mut records = self.0.lock().unwrap();
        let index = records.len();
        records.push(CallRecord {
            interface: interface.to_owned(),
            function: call.function.to_owned(),
            handles: call.handles.to_vec(),
            produced: Vec::new(),
        });
        Ok(Some(index))
    }

    fn after(&self, _: &mut State<H>, _: &Call<'_>, frame: Option<usize>, outcome: Outcome<'_>) {
        if let (Some(index), Outcome::Returned(completion)) = (frame, outcome) {
            self.0.lock().unwrap()[index]
                .produced
                .clone_from(&completion.produced);
        }
    }
}

async fn server(
    message: &'static str,
    connections: usize,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let authority = listener.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move {
        for _ in 0..connections {
            let (stream, _) = listener.accept().await.unwrap();
            http1::Builder::new()
                .serve_connection(
                    TokioIo::new(stream),
                    service_fn(move |_| async move {
                        Ok::<_, Infallible>(
                            http::Response::builder()
                                .status(201)
                                .header("x-server", "loopback")
                                .body(Full::new(Bytes::from_static(message.as_bytes())))
                                .unwrap(),
                        )
                    }),
                )
                .await
                .unwrap();
        }
    });
    (authority, task)
}

fn engine() -> Engine {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    Engine::new(&config).unwrap()
}

fn state<H: Send>(chain: Arc<Chain<State<H>>>, hooks: H) -> State<H> {
    state_with_socket_policy(chain, hooks, Arc::new(AtomicBool::new(false)), true)
}

fn state_with_socket_policy<H: Send>(
    chain: Arc<Chain<State<H>>>,
    hooks: H,
    checked: Arc<AtomicBool>,
    allowed: bool,
) -> State<H> {
    let mut wasi = WasiCtxBuilder::new();
    wasi.allow_tcp(true).socket_addr_check(move |_, _| {
        checked.store(true, Ordering::Relaxed);
        Box::pin(async move { allowed })
    });
    State {
        middleware: MiddlewareCtx::new(chain, InvocationContext::new("http-client")),
        table: ResourceTable::new(),
        wasi: wasi.build(),
        http: WasiHttpCtx::new(),
        hooks,
    }
}

async fn fetch_p2(authority: &str, gated: bool) -> (String, Vec<CallRecord>) {
    let engine = engine();
    let component = Component::from_file(&engine, test_guests::http_p2()).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let chain = Chain::builder().layer(Record(Arc::clone(&calls))).build();
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker).unwrap();
    if gated {
        wasm_component_middleware_wasi_http::p2::add_only_http_to_linker_async(&mut linker)
            .unwrap();
    } else {
        wasmtime_wasi_http::p2::add_only_http_to_linker_async(&mut linker).unwrap();
    }
    let mut store = Store::new(&engine, state(chain, DefaultHooks));
    let guest = p2::Client::instantiate_async(&mut store, &component, &linker)
        .await
        .unwrap();
    let result = guest
        .call_fetch(&mut store, authority)
        .await
        .unwrap()
        .unwrap();
    let observed = calls.lock().unwrap().clone();
    (result, observed)
}

async fn fetch_p3(authority: &str, gated: bool) -> (String, Vec<CallRecord>) {
    let engine = engine();
    let component = Component::from_file(&engine, test_guests::http_p3()).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let chain = Chain::builder().layer(Record(Arc::clone(&calls))).build();
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker).unwrap();
    if gated {
        wasm_component_middleware_wasi_http::p3::add_to_linker(&mut linker).unwrap();
    } else {
        wasmtime_wasi_http::p3::add_to_linker(&mut linker).unwrap();
    }
    let mut store = Store::new(&engine, state(chain, DefaultHooks));
    let guest = p3::Client::instantiate_async(&mut store, &component, &linker)
        .await
        .unwrap();
    let result = store
        .run_concurrent(async |accessor| guest.call_fetch(accessor, authority.to_owned()).await)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let observed = calls.lock().unwrap().clone();
    (result, observed)
}

fn assert_resource_tracking(records: &[CallRecord]) {
    let mut produced = BTreeSet::<u32>::new();
    for record in records {
        for handle in &record.handles {
            assert!(
                produced.contains(handle),
                "{}.{} used unreported handle {handle}; records: {records:#?}",
                record.interface,
                record.function
            );
        }
        produced.extend(&record.produced);
    }

    for record in records
        .iter()
        .filter(|record| record.function.starts_with("[resource-drop]"))
    {
        assert!(!record.handles.is_empty(), "{record:?}");
        assert!(
            record
                .handles
                .iter()
                .all(|handle| produced.contains(handle)),
            "drop did not match a produced resource: {record:?}"
        );
    }
}

fn expected_calls(wit: &str) -> BTreeSet<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let routed = if wit == "wit" {
        wasm_component_middleware_wasi_http::p2::ROUTED_INTERFACES
    } else {
        wasm_component_middleware_wasi_http::p3::ROUTED_INTERFACES
    };
    let mut resolve = Resolve::default();
    resolve
        .push_dir(root.join(if wit == "wit" {
            "../wasm-component-middleware-wasi/wit"
        } else {
            "../wasm-component-middleware-wasi/wit-p3"
        }))
        .unwrap();
    resolve.push_dir(root.join(wit)).unwrap();
    let mut expected = BTreeSet::new();

    for (_, package) in &resolve.packages {
        if package.name.namespace != "wasi" || package.name.name != "http" {
            continue;
        }
        for (name, interface_id) in &package.interfaces {
            let interface_name = format!("wasi:http/{name}");
            if !routed.iter().any(|routed| routed.name() == interface_name) {
                continue;
            }
            let interface = &resolve.interfaces[*interface_id];
            expected.extend(
                interface
                    .functions
                    .values()
                    .map(|function| (interface_name.clone(), function.name.clone())),
            );
            expected.extend(
                interface
                    .types
                    .iter()
                    .filter(|(_, id)| matches!(resolve.types[**id].kind, TypeDefKind::Resource))
                    .map(|(resource, _)| {
                        (interface_name.clone(), format!("[resource-drop]{resource}"))
                    }),
            );
        }
    }
    expected
}

fn assert_wit_dispatch_coverage(
    records: &[CallRecord],
    wit: &str,
    unavailable: &[(&str, &str, &str)],
) {
    let observed = records
        .iter()
        .filter(|record| record.interface.starts_with("wasi:http/"))
        .map(|record| (record.interface.clone(), record.function.clone()))
        .collect::<BTreeSet<_>>();
    let mut expected = expected_calls(wit);
    for (interface, function, _reason) in unavailable {
        assert!(expected.remove(&(String::from(*interface), String::from(*function))));
    }
    assert_eq!(observed, expected);
}

fn p2_unavailable_calls() -> &'static [(&'static str, &'static str, &'static str)] {
    const TYPES: &str = "wasi:http/types";
    &[
        (TYPES, "[constructor]outgoing-response", "service-side API"),
        (
            TYPES,
            "[method]incoming-request.authority",
            "service-side API",
        ),
        (
            TYPES,
            "[method]incoming-request.consume",
            "service-side API",
        ),
        (
            TYPES,
            "[method]incoming-request.headers",
            "service-side API",
        ),
        (TYPES, "[method]incoming-request.method", "service-side API"),
        (
            TYPES,
            "[method]incoming-request.path-with-query",
            "service-side API",
        ),
        (TYPES, "[method]incoming-request.scheme", "service-side API"),
        (TYPES, "[method]outgoing-response.body", "service-side API"),
        (
            TYPES,
            "[method]outgoing-response.headers",
            "service-side API",
        ),
        (
            TYPES,
            "[method]outgoing-response.set-status-code",
            "service-side API",
        ),
        (
            TYPES,
            "[method]outgoing-response.status-code",
            "service-side API",
        ),
        (
            TYPES,
            "[resource-drop]incoming-body",
            "round trip consumes the body with finish",
        ),
        (TYPES, "[resource-drop]incoming-request", "service-side API"),
        (
            TYPES,
            "[resource-drop]outgoing-body",
            "round trip consumes the body with finish",
        ),
        (
            TYPES,
            "[resource-drop]outgoing-request",
            "round trip transfers the request to handle",
        ),
        (
            TYPES,
            "[resource-drop]outgoing-response",
            "service-side API",
        ),
        (
            TYPES,
            "[resource-drop]request-options",
            "round trip transfers the options to handle",
        ),
        (
            TYPES,
            "[resource-drop]response-outparam",
            "service-side API",
        ),
        (TYPES, "[static]response-outparam.set", "service-side API"),
        (
            TYPES,
            "http-error-code",
            "requires a wasi:io error resource",
        ),
    ]
}

fn p3_unavailable_calls() -> &'static [(&'static str, &'static str, &'static str)] {
    const TYPES: &str = "wasi:http/types";
    &[
        (
            TYPES,
            "[resource-drop]request",
            "round trip transfers the request to send",
        ),
        (
            TYPES,
            "[resource-drop]response",
            "round trip consumes the response body",
        ),
        (
            TYPES,
            "[method]response.set-status-code",
            "service-side mutation",
        ),
        (TYPES, "[static]request.consume-body", "service-side API"),
        (TYPES, "[static]response.new", "service-side API"),
    ]
}

#[tokio::test]
async fn preview_2_gated_and_plain_requests_match() {
    let (authority, server) = server("hello from p2", 2).await;
    let (plain, _) = fetch_p2(&authority, false).await;
    let (gated, calls) = fetch_p2(&authority, true).await;
    server.await.unwrap();

    assert_eq!(
        plain,
        format!(
            "GET http://{authority}/message x-client=middleware -> 201 x-server=loopback | hello from p2"
        )
    );
    assert_eq!(gated, plain);
    assert_resource_tracking(&calls);
    assert_wit_dispatch_coverage(&calls, "wit", p2_unavailable_calls());
}

#[tokio::test]
async fn preview_3_gated_and_plain_requests_match() {
    let (authority, server) = server("hello from p3", 2).await;
    let (plain, _) = fetch_p3(&authority, false).await;
    let (gated, calls) = fetch_p3(&authority, true).await;
    server.await.unwrap();

    assert_eq!(
        plain,
        format!(
            "GET http://{authority}/message x-client=middleware -> 201 x-server=loopback | hello from p3"
        )
    );
    assert_eq!(gated, plain);
    assert_resource_tracking(&calls);
    assert_wit_dispatch_coverage(&calls, "wit-p3", p3_unavailable_calls());
}

struct HookState {
    middleware: MiddlewareCtx<Self>,
}

impl MiddlewareView for HookState {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        &mut self.middleware
    }
}

struct DenyAuthority(String);

impl Layer<HookState> for DenyAuthority {
    type Frame = ();

    fn before(&self, _: &mut HookState, call: &Call<'_>) -> Result<(), Denied> {
        if call.args.get("authority").and_then(ArgumentValue::as_str) == Some(self.0.as_str()) {
            Err(Denied::new("authority is not allowed"))
        } else {
            Ok(())
        }
    }

    fn after(&self, _: &mut HookState, _: &Call<'_>, (): (), _: Outcome<'_>) {}
}

struct AllowAuthority(String);

impl Layer<HookState> for AllowAuthority {
    type Frame = ();

    fn before(&self, _: &mut HookState, call: &Call<'_>) -> Result<(), Denied> {
        if call.args.get("authority").and_then(ArgumentValue::as_str) == Some(self.0.as_str()) {
            Ok(())
        } else {
            Err(Denied::new("authority is not allowed"))
        }
    }

    fn after(&self, _: &mut HookState, _: &Call<'_>, (): (), _: Outcome<'_>) {}
}

#[tokio::test]
async fn http_hook_denies_before_an_allowed_socket_connection() {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let authority = listener.local_addr().unwrap().to_string();
    let accepted = Arc::new(AtomicBool::new(false));
    let accepted_task = Arc::clone(&accepted);
    let server = tokio::spawn(async move {
        if tokio::time::timeout(std::time::Duration::from_millis(200), listener.accept())
            .await
            .is_ok()
        {
            accepted_task.store(true, Ordering::Relaxed);
        }
    });
    let hook_chain = Chain::builder()
        .layer(DenyAuthority(authority.clone()))
        .build();
    let hook_state = HookState {
        middleware: MiddlewareCtx::new(hook_chain, InvocationContext::new("http-policy")),
    };
    let hooks = WasiHttpHooks::new(hook_state, DefaultHooks);
    let engine = engine();
    let component = Component::from_file(&engine, test_guests::http_p2()).unwrap();
    let chain = Chain::builder().build();
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker).unwrap();
    wasm_component_middleware_wasi_http::p2::add_only_http_to_linker_async(&mut linker).unwrap();
    let mut store = Store::new(&engine, state(chain, hooks));
    let guest = p2::Client::instantiate_async(&mut store, &component, &linker)
        .await
        .unwrap();

    let error = guest
        .call_fetch(&mut store, &authority)
        .await
        .unwrap()
        .unwrap_err();
    server.await.unwrap();

    assert!(error.contains("HttpRequestDenied"), "{error}");
    assert!(!accepted.load(Ordering::Relaxed));
}

#[tokio::test]
async fn allowed_http_bypasses_a_restrictive_socket_policy() {
    let (authority, server) = server("socket policy was bypassed", 1).await;
    let hook_chain = Chain::builder()
        .layer(AllowAuthority(authority.clone()))
        .build();
    let hook_state = HookState {
        middleware: MiddlewareCtx::new(hook_chain, InvocationContext::new("http-policy")),
    };
    let hooks = WasiHttpHooks::new(hook_state, DefaultHooks);
    let engine = engine();
    let component = Component::from_file(&engine, test_guests::http_p2()).unwrap();
    let chain = Chain::builder().build();
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker).unwrap();
    wasm_component_middleware_wasi_http::p2::add_only_http_to_linker_async(&mut linker).unwrap();
    let socket_check = Arc::new(AtomicBool::new(false));
    let state = state_with_socket_policy(chain, hooks, Arc::clone(&socket_check), false);
    let mut store = Store::new(&engine, state);
    let guest = p2::Client::instantiate_async(&mut store, &component, &linker)
        .await
        .unwrap();

    let result = guest
        .call_fetch(&mut store, &authority)
        .await
        .unwrap()
        .unwrap();
    server.await.unwrap();

    assert!(result.ends_with("| socket policy was bypassed"), "{result}");
    assert!(!socket_check.load(Ordering::Relaxed));
}
