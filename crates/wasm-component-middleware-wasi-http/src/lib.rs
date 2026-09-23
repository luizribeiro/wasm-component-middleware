//! Middleware gates and hooks for outgoing WASI HTTP requests.
//!
//! Use this crate to apply component-call middleware to the WASI HTTP adapter
//! while sharing the core and WASI integration crates.

mod gate;
pub mod p2;

use std::sync::{Arc, Mutex, MutexGuard};

use http::{HeaderName, uri::Scheme};
use wasm_component_middleware::{
    ArgumentValue, Arguments, Call, Completion, Direction, MiddlewareView, StateAccess,
};
use wasmtime_wasi_http::{Error, RequestOptions, WasiBody, WasiHttpHooks as WasmtimeWasiHttpHooks};

/// The synthetic function name used for complete outgoing requests.
pub const SEND_REQUEST: &str = "[send-request]";

/// The pseudo-interface used for complete outgoing request hooks.
///
/// Wasmtime uses the same hook for Preview 2 and Preview 3, so this name is
/// deliberately outside both versions' real interface sets.
pub const REQUEST_HOOK_INTERFACE: &str = "wasi:http/request-hook";

/// Default Wasmtime behavior for [`WasiHttpHooks`].
#[derive(Default)]
pub struct DefaultHooks;

impl WasmtimeWasiHttpHooks for DefaultHooks {}

/// Applies a middleware chain to complete outgoing HTTP requests.
///
/// Store this beside [`wasmtime_wasi_http::WasiHttpCtx`] and return it from
/// [`wasmtime_wasi_http::WasiHttpView::http`] as the view's `hooks` field.
/// The policy state and delegate are locked only while synchronous hooks run;
/// neither lock is held while an HTTP request is in flight.
///
/// This hook runs an independent middleware chain. It has separate state and
/// call identifiers from the store's chain, and calls in the two chains are
/// not correlated. For Preview 2, the store chain's
/// `wasi:http/outgoing-handler.handle` call returns before this hook decides
/// because Wasmtime sends the request from a spawned task. Put an [`Arc`] (or
/// another shared value) in layers installed in both chains when they need to
/// share policy or observations.
pub struct WasiHttpHooks<S, H = DefaultHooks> {
    state: Arc<Mutex<S>>,
    delegate: Arc<Mutex<H>>,
}

impl<S, H> WasiHttpHooks<S, H> {
    /// Wraps `delegate` with middleware using the supplied policy state.
    ///
    /// The state's [`MiddlewareView`] selects the chain and invocation context
    /// used for the synthetic [`SEND_REQUEST`] call.
    pub fn new(state: S, delegate: H) -> Self {
        Self {
            state: Arc::new(Mutex::new(state)),
            delegate: Arc::new(Mutex::new(delegate)),
        }
    }

    fn delegate(&self) -> MutexGuard<'_, H> {
        self.delegate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

struct SharedState<S>(Arc<Mutex<S>>);

impl<S> StateAccess<S> for SharedState<S> {
    fn with_state<R>(&mut self, operation: impl FnOnce(&mut S) -> R) -> R {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        operation(&mut state)
    }
}

fn request_arguments(request: &http::Request<WasiBody>) -> Arguments {
    let uri = request.uri();
    let headers = request
        .headers()
        .iter()
        .map(|(name, value)| {
            ArgumentValue::from(format!(
                "{name}: {}",
                String::from_utf8_lossy(value.as_bytes())
            ))
        })
        .collect();
    Arguments::new()
        .with("method", request.method().as_str())
        .with("scheme", uri.scheme_str().unwrap_or_default())
        .with(
            "authority",
            uri.authority().map_or("", http::uri::Authority::as_str),
        )
        .with(
            "path",
            uri.path_and_query()
                .map_or("", http::uri::PathAndQuery::as_str),
        )
        .with("headers", ArgumentValue::List(headers))
}

fn http_error(error: wasmtime::Error) -> Error {
    match error.downcast::<wasm_component_middleware::Denied>() {
        Ok(_) => Error::HttpRequestDenied,
        Err(error) => match error.downcast::<Error>() {
            Ok(error) => error,
            Err(error) => Error::InternalError(Some(error.to_string())),
        },
    }
}

impl<S, H> WasmtimeWasiHttpHooks for WasiHttpHooks<S, H>
where
    S: MiddlewareView + 'static,
    H: WasmtimeWasiHttpHooks + 'static,
{
    fn is_forbidden_header(&mut self, name: &HeaderName) -> bool {
        self.delegate().is_forbidden_header(name)
    }

    fn is_supported_scheme(&mut self, scheme: &Scheme) -> bool {
        self.delegate().is_supported_scheme(scheme)
    }

    fn set_host_header(&mut self) -> bool {
        self.delegate().set_host_header()
    }

    fn default_scheme(&mut self) -> Option<Scheme> {
        self.delegate().default_scheme()
    }

    fn send_request(
        &mut self,
        request: http::Request<WasiBody>,
        options: Option<RequestOptions>,
        fut: Box<dyn Future<Output = Result<(), Error>> + Send>,
    ) -> Box<
        dyn Future<
                Output = Result<
                    (
                        http::Response<WasiBody>,
                        Box<dyn Future<Output = Result<(), Error>> + Send>,
                    ),
                    Error,
                >,
            > + Send,
    > {
        let arguments = request_arguments(&request);
        let state = SharedState(Arc::clone(&self.state));
        let delegate = Arc::clone(&self.delegate);
        let chain = state.0.lock().map_or_else(
            |poisoned| Arc::clone(poisoned.into_inner().middleware().chain()),
            |mut state| Arc::clone(state.middleware().chain()),
        );
        Box::new(async move {
            let call = Call::new(chain.next_id(), Direction::Import, SEND_REQUEST)
                .in_interface(REQUEST_HOOK_INTERFACE, None)
                .with_args(&arguments);
            chain
                .dispatch_async(state, &call, || async move {
                    let request = delegate
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .send_request(request, options, fut);
                    std::pin::Pin::from(request)
                        .await
                        .map(|response| (response, Completion::default()))
                        .map_err(wasmtime::Error::new)
                })
                .await
                .map_err(http_error)
        })
    }

    fn p2_outgoing_body_buffer_chunks(&mut self) -> usize {
        self.delegate().p2_outgoing_body_buffer_chunks()
    }

    fn p2_outgoing_body_chunk_size(&mut self) -> usize {
        self.delegate().p2_outgoing_body_chunk_size()
    }

    fn p2_error_from_hyper(
        &mut self,
        error: &hyper::Error,
    ) -> wasmtime_wasi_http::p2::bindings::http::types::ErrorCode {
        self.delegate().p2_error_from_hyper(error)
    }

    fn p2_error_from_connect(
        &mut self,
        error: &std::io::Error,
    ) -> wasmtime_wasi_http::p2::bindings::http::types::ErrorCode {
        self.delegate().p2_error_from_connect(error)
    }

    fn p2_error_from_tls(
        &mut self,
        error: &std::io::Error,
    ) -> wasmtime_wasi_http::p2::bindings::http::types::ErrorCode {
        self.delegate().p2_error_from_tls(error)
    }

    fn p2_error_from_dns(
        &mut self,
        error: &rustls::pki_types::InvalidDnsNameError,
    ) -> wasmtime_wasi_http::p2::bindings::http::types::ErrorCode {
        self.delegate().p2_error_from_dns(error)
    }

    fn p3_outgoing_body_chunk_size(&mut self) -> usize {
        self.delegate().p3_outgoing_body_chunk_size()
    }

    fn p3_error_from_hyper(
        &mut self,
        error: &hyper::Error,
    ) -> wasmtime_wasi_http::p3::bindings::http::types::ErrorCode {
        self.delegate().p3_error_from_hyper(error)
    }

    fn p3_error_from_connect(
        &mut self,
        error: &std::io::Error,
    ) -> wasmtime_wasi_http::p3::bindings::http::types::ErrorCode {
        self.delegate().p3_error_from_connect(error)
    }

    fn p3_error_from_tls(
        &mut self,
        error: &std::io::Error,
    ) -> wasmtime_wasi_http::p3::bindings::http::types::ErrorCode {
        self.delegate().p3_error_from_tls(error)
    }

    fn p3_error_from_dns(
        &mut self,
        error: &rustls::pki_types::InvalidDnsNameError,
    ) -> wasmtime_wasi_http::p3::bindings::http::types::ErrorCode {
        self.delegate().p3_error_from_dns(error)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::task::{Context, Poll};

    use bytes::Bytes;
    use http_body_util::{BodyExt, Empty};
    use hyper::server::conn::http1;
    use hyper::service::service_fn;
    use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
    use wasm_component_middleware::{
        Chain, Denied, InvocationContext, Layer, MiddlewareCtx, Outcome,
    };
    use wasmtime_wasi_http::io::TokioIo;

    use super::*;

    struct State {
        middleware: Option<MiddlewareCtx<Self>>,
        seen: Arc<Mutex<Option<Arguments>>>,
        deny: bool,
    }

    impl MiddlewareView for State {
        fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
            self.middleware.as_mut().unwrap()
        }
    }

    struct Policy;

    impl Layer<State> for Policy {
        type Frame = ();

        fn before(&self, state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
            assert_eq!(call.function, SEND_REQUEST);
            *state.seen.lock().unwrap() = Some(call.args.clone());
            if state.deny {
                Err(Denied::new("authority is not allowed"))
            } else {
                Ok(())
            }
        }

        fn after(&self, _: &mut State, _: &Call<'_>, (): (), _: Outcome<'_>) {}
    }

    struct Delegate(Arc<AtomicBool>);

    impl WasmtimeWasiHttpHooks for Delegate {
        fn send_request(
            &mut self,
            _: http::Request<WasiBody>,
            _: Option<RequestOptions>,
            _: Box<dyn Future<Output = Result<(), Error>> + Send>,
        ) -> Box<
            dyn Future<
                    Output = Result<
                        (
                            http::Response<WasiBody>,
                            Box<dyn Future<Output = Result<(), Error>> + Send>,
                        ),
                        Error,
                    >,
                > + Send,
        > {
            self.0.store(true, Ordering::Relaxed);
            Box::new(async { Err(Error::ConnectionRefused) })
        }
    }

    struct TestHooks {
        hooks: WasiHttpHooks<State, Delegate>,
        called: Arc<AtomicBool>,
        seen: Arc<Mutex<Option<Arguments>>>,
    }

    fn hooks(deny: bool) -> TestHooks {
        let called = Arc::new(AtomicBool::new(false));
        let seen = Arc::new(Mutex::new(None));
        let chain = Chain::builder().layer(Policy).build();
        let state = State {
            middleware: Some(MiddlewareCtx::new(
                chain,
                InvocationContext::new("http-test"),
            )),
            seen: Arc::clone(&seen),
            deny,
        };
        TestHooks {
            hooks: WasiHttpHooks::new(state, Delegate(Arc::clone(&called))),
            called,
            seen,
        }
    }

    fn request() -> http::Request<WasiBody> {
        http::Request::builder()
            .method("POST")
            .uri("http://api.example.test/v1/items?fresh=true")
            .header("x-policy", "inspect-me")
            .body(
                Empty::<Bytes>::new()
                    .map_err(|error: Infallible| match error {})
                    .boxed_unsync(),
            )
            .unwrap()
    }

    #[tokio::test]
    async fn request_metadata_is_visible_and_the_delegate_runs() {
        let TestHooks {
            mut hooks,
            called,
            seen,
        } = hooks(false);

        let result =
            std::pin::Pin::from(hooks.send_request(request(), None, Box::new(async { Ok(()) })))
                .await;

        assert!(matches!(result, Err(Error::ConnectionRefused)));
        assert!(called.load(Ordering::Relaxed));
        let seen = seen.lock().unwrap();
        let args = seen.as_ref().unwrap();
        assert_eq!(
            args.get("method").and_then(ArgumentValue::as_str),
            Some("POST")
        );
        assert_eq!(
            args.get("scheme").and_then(ArgumentValue::as_str),
            Some("http")
        );
        assert_eq!(
            args.get("authority").and_then(ArgumentValue::as_str),
            Some("api.example.test")
        );
        assert_eq!(
            args.get("path").and_then(ArgumentValue::as_str),
            Some("/v1/items?fresh=true")
        );
        assert_eq!(
            args.get("headers").unwrap().to_string(),
            "[\"x-policy: inspect-me\"]"
        );
    }

    #[tokio::test]
    async fn refusal_becomes_the_wasi_http_denial_error() {
        let TestHooks {
            mut hooks, called, ..
        } = hooks(true);

        let result =
            std::pin::Pin::from(hooks.send_request(request(), None, Box::new(async { Ok(()) })))
                .await;

        assert!(matches!(result, Err(Error::HttpRequestDenied)));
        assert!(!called.load(Ordering::Relaxed));
    }

    struct ForwardingDelegate(Arc<Mutex<Vec<&'static str>>>);

    impl ForwardingDelegate {
        fn record(&self, method: &'static str) {
            self.0.lock().unwrap().push(method);
        }
    }

    impl WasmtimeWasiHttpHooks for ForwardingDelegate {
        fn is_forbidden_header(&mut self, _: &HeaderName) -> bool {
            self.record("is_forbidden_header");
            true
        }

        fn is_supported_scheme(&mut self, _: &Scheme) -> bool {
            self.record("is_supported_scheme");
            false
        }

        fn set_host_header(&mut self) -> bool {
            self.record("set_host_header");
            false
        }

        fn default_scheme(&mut self) -> Option<Scheme> {
            self.record("default_scheme");
            Some(Scheme::HTTP)
        }

        fn p2_outgoing_body_buffer_chunks(&mut self) -> usize {
            self.record("p2_outgoing_body_buffer_chunks");
            7
        }

        fn p2_outgoing_body_chunk_size(&mut self) -> usize {
            self.record("p2_outgoing_body_chunk_size");
            11
        }

        fn p2_error_from_hyper(
            &mut self,
            _: &hyper::Error,
        ) -> wasmtime_wasi_http::p2::bindings::http::types::ErrorCode {
            self.record("p2_error_from_hyper");
            wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::HttpProtocolError
        }

        fn p2_error_from_connect(
            &mut self,
            _: &std::io::Error,
        ) -> wasmtime_wasi_http::p2::bindings::http::types::ErrorCode {
            self.record("p2_error_from_connect");
            wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::ConnectionRefused
        }

        fn p2_error_from_tls(
            &mut self,
            _: &std::io::Error,
        ) -> wasmtime_wasi_http::p2::bindings::http::types::ErrorCode {
            self.record("p2_error_from_tls");
            wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::TlsProtocolError
        }

        fn p2_error_from_dns(
            &mut self,
            _: &rustls::pki_types::InvalidDnsNameError,
        ) -> wasmtime_wasi_http::p2::bindings::http::types::ErrorCode {
            self.record("p2_error_from_dns");
            wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::DnsTimeout
        }

        fn p3_outgoing_body_chunk_size(&mut self) -> usize {
            self.record("p3_outgoing_body_chunk_size");
            13
        }

        fn p3_error_from_hyper(
            &mut self,
            _: &hyper::Error,
        ) -> wasmtime_wasi_http::p3::bindings::http::types::ErrorCode {
            self.record("p3_error_from_hyper");
            wasmtime_wasi_http::p3::bindings::http::types::ErrorCode::HttpProtocolError
        }

        fn p3_error_from_connect(
            &mut self,
            _: &std::io::Error,
        ) -> wasmtime_wasi_http::p3::bindings::http::types::ErrorCode {
            self.record("p3_error_from_connect");
            wasmtime_wasi_http::p3::bindings::http::types::ErrorCode::ConnectionRefused
        }

        fn p3_error_from_tls(
            &mut self,
            _: &std::io::Error,
        ) -> wasmtime_wasi_http::p3::bindings::http::types::ErrorCode {
            self.record("p3_error_from_tls");
            wasmtime_wasi_http::p3::bindings::http::types::ErrorCode::TlsProtocolError
        }

        fn p3_error_from_dns(
            &mut self,
            _: &rustls::pki_types::InvalidDnsNameError,
        ) -> wasmtime_wasi_http::p3::bindings::http::types::ErrorCode {
            self.record("p3_error_from_dns");
            wasmtime_wasi_http::p3::bindings::http::types::ErrorCode::DnsTimeout
        }
    }

    struct InvalidHttp(bool);

    impl AsyncRead for InvalidHttp {
        fn poll_read(
            mut self: Pin<&mut Self>,
            _: &mut Context<'_>,
            buffer: &mut ReadBuf<'_>,
        ) -> Poll<std::io::Result<()>> {
            if self.0 {
                Poll::Ready(Ok(()))
            } else {
                self.0 = true;
                buffer.put_slice(b"invalid HTTP\r\n\r\n");
                Poll::Ready(Ok(()))
            }
        }
    }

    impl AsyncWrite for InvalidHttp {
        fn poll_write(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &[u8],
        ) -> Poll<Result<usize, std::io::Error>> {
            Poll::Ready(Err(std::io::Error::other("test error")))
        }

        fn poll_flush(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            Poll::Ready(Ok(()))
        }

        fn poll_shutdown(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
        ) -> Poll<Result<(), std::io::Error>> {
            Poll::Ready(Ok(()))
        }
    }

    async fn hyper_error() -> hyper::Error {
        http1::Builder::new()
            .serve_connection(
                TokioIo::new(InvalidHttp(false)),
                service_fn(|_| async {
                    Ok::<_, Infallible>(http::Response::new(Empty::<Bytes>::new()))
                }),
            )
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn every_other_hook_method_forwards_to_the_delegate() {
        let methods = Arc::new(Mutex::new(Vec::new()));
        let chain = Chain::builder().build();
        let state = State {
            middleware: Some(MiddlewareCtx::new(
                chain,
                InvocationContext::new("http-test"),
            )),
            seen: Arc::new(Mutex::new(None)),
            deny: false,
        };
        let mut hooks = WasiHttpHooks::new(state, ForwardingDelegate(Arc::clone(&methods)));
        let hyper = hyper_error().await;
        let io = std::io::Error::other("test error");
        let dns = rustls::pki_types::ServerName::try_from("bad name").unwrap_err();

        assert!(hooks.is_forbidden_header(&http::header::AUTHORIZATION));
        assert!(!hooks.is_supported_scheme(&Scheme::HTTP));
        assert!(!hooks.set_host_header());
        assert_eq!(hooks.default_scheme(), Some(Scheme::HTTP));
        assert_eq!(hooks.p2_outgoing_body_buffer_chunks(), 7);
        assert_eq!(hooks.p2_outgoing_body_chunk_size(), 11);
        assert!(matches!(
            hooks.p2_error_from_hyper(&hyper),
            wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::HttpProtocolError
        ));
        assert!(matches!(
            hooks.p2_error_from_connect(&io),
            wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::ConnectionRefused
        ));
        assert!(matches!(
            hooks.p2_error_from_tls(&io),
            wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::TlsProtocolError
        ));
        assert!(matches!(
            hooks.p2_error_from_dns(&dns),
            wasmtime_wasi_http::p2::bindings::http::types::ErrorCode::DnsTimeout
        ));
        assert_eq!(hooks.p3_outgoing_body_chunk_size(), 13);
        assert!(matches!(
            hooks.p3_error_from_hyper(&hyper),
            wasmtime_wasi_http::p3::bindings::http::types::ErrorCode::HttpProtocolError
        ));
        assert!(matches!(
            hooks.p3_error_from_connect(&io),
            wasmtime_wasi_http::p3::bindings::http::types::ErrorCode::ConnectionRefused
        ));
        assert!(matches!(
            hooks.p3_error_from_tls(&io),
            wasmtime_wasi_http::p3::bindings::http::types::ErrorCode::TlsProtocolError
        ));
        assert!(matches!(
            hooks.p3_error_from_dns(&dns),
            wasmtime_wasi_http::p3::bindings::http::types::ErrorCode::DnsTimeout
        ));

        assert_eq!(
            *methods.lock().unwrap(),
            [
                "is_forbidden_header",
                "is_supported_scheme",
                "set_host_header",
                "default_scheme",
                "p2_outgoing_body_buffer_chunks",
                "p2_outgoing_body_chunk_size",
                "p2_error_from_hyper",
                "p2_error_from_connect",
                "p2_error_from_tls",
                "p2_error_from_dns",
                "p3_outgoing_body_chunk_size",
                "p3_error_from_hyper",
                "p3_error_from_connect",
                "p3_error_from_tls",
                "p3_error_from_dns",
            ]
        );
    }
}
