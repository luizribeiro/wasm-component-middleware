#![allow(missing_docs)]

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::Full;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use wasm_component_middleware::{
    ArgumentValue, Call, Chain, Denied, InvocationContext, Layer, Logger, MiddlewareCtx,
    MiddlewareView, Outcome,
};
use wasm_component_middleware_wasi_http::{DefaultHooks, REQUEST_HOOK_INTERFACE, WasiHttpHooks};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::io::TokioIo;
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};

wasmtime::component::bindgen!({
    path: "guest/wit",
    world: "client",
    exports: { default: async },
    require_store_data_send: true,
});

struct HttpPolicy {
    middleware: MiddlewareCtx<Self>,
}

impl MiddlewareView for HttpPolicy {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        &mut self.middleware
    }
}

#[derive(Clone)]
struct AllowAuthority(Arc<str>);

impl<T> Layer<T> for AllowAuthority {
    type Frame = ();

    fn before(&self, _: &mut T, call: &Call<'_>) -> Result<(), Denied> {
        if call.interface != Some(REQUEST_HOOK_INTERFACE) {
            return Ok(());
        }
        let authority = call.args.get("authority").and_then(ArgumentValue::as_str);
        if authority == Some(self.0.as_ref()) {
            Ok(())
        } else {
            Err(Denied::new("authority is not allowed"))
        }
    }

    fn after(&self, _: &mut T, _: &Call<'_>, (): (), _: Outcome<'_>) {}
}

type Hooks = WasiHttpHooks<HttpPolicy>;

struct State {
    middleware: MiddlewareCtx<Self>,
    table: ResourceTable,
    wasi: WasiCtx,
    http: WasiHttpCtx,
    hooks: Hooks,
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

async fn serve(message: &'static str) -> (String, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let authority = listener.local_addr().unwrap().to_string();
    let task = tokio::spawn(async move {
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
    });
    (authority, task)
}

#[tokio::main]
async fn main() -> wasmtime::Result<()> {
    let (allowed, allowed_server) = serve("hello from the allowed server").await;
    let (denied, denied_server) = serve("this response must not be reached").await;
    let authority_policy = AllowAuthority(Arc::from(allowed.as_str()));
    let policy_chain = Chain::builder()
        .layer(Logger::stderr())
        .layer(authority_policy.clone())
        .build();
    let policy = HttpPolicy {
        middleware: MiddlewareCtx::new(policy_chain, InvocationContext::new("http-client")),
    };

    let mut config = Config::new();
    config.wasm_component_model_async(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, guest_build::example("http-allowlist"))?;
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker)?;
    wasm_component_middleware_wasi_http::p2::add_only_http_to_linker_async(&mut linker)?;
    let chain = Chain::builder().layer(authority_policy).build();
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("http-client")),
            table: ResourceTable::new(),
            wasi: WasiCtxBuilder::new().build(),
            http: WasiHttpCtx::new(),
            hooks: WasiHttpHooks::new(policy, DefaultHooks),
        },
    );
    let guest = Client::instantiate_async(&mut store, &component, &linker).await?;

    let body = guest
        .call_fetch(&mut store, &allowed)
        .await?
        .map_err(wasmtime::Error::msg)?;
    println!("allowed body: {body}");
    let error = guest.call_fetch(&mut store, &denied).await?.unwrap_err();
    println!("denied error: {error}");

    allowed_server.abort();
    denied_server.abort();
    Ok(())
}
