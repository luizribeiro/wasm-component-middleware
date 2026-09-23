//! Middleware gates for WASI HTTP Preview 3 interfaces.

use wasm_component_middleware::{MiddlewareView, RoutedInterface};
use wasmtime::AsContextMut;
use wasmtime::component::{Access, Accessor, FutureReader, Linker, Resource, StreamReader};
use wasmtime_wasi::TrappableError;
use wasmtime_wasi_http::p3::bindings::clocks::monotonic_clock::Duration;
use wasmtime_wasi_http::p3::bindings::http::{client, types};
use wasmtime_wasi_http::p3::{Request, Response};
use wasmtime_wasi_http::{FieldMap, WasiHttp, WasiHttpView};

use crate::gate::{Gate, GateData, gate, project, trappable_error, typed_error};

const VERSION: &str = "0.3.0";

type HttpError = TrappableError<types::ErrorCode>;
type HeaderError = TrappableError<types::HeaderError>;
type OptionsError = TrappableError<types::RequestOptionsError>;

/// Interfaces routed by [`add_to_linker`].
pub const ROUTED_INTERFACES: &[RoutedInterface] = &[
    RoutedInterface::from_static("wasi:http/client"),
    RoutedInterface::from_static("wasi:http/types"),
];

macro_rules! call {
    ($kind:ident $self:ident, $function:literal, [$($handle:expr),*], $args:tt, $delegate:expr $(, $produced:expr)?) => {{
        let result = gate!(trap $self, VERSION, "wasi:http/types", $function, handles = [$($handle),*], args = $args, delegate = |state| call!(@delegate $kind $delegate, state) $(, produced = $produced)?);
        call!(@result $kind result)
    }};
    (@delegate trap $delegate:expr, $state:ident) => { ($delegate)($state) };
    (@delegate http $delegate:expr, $state:ident) => { ($delegate)($state).map_err(|error| trappable_error(error, TrappableError::downcast)) };
    (@delegate header $delegate:expr, $state:ident) => { ($delegate)($state).map_err(|error| trappable_error(error, TrappableError::downcast)) };
    (@delegate options $delegate:expr, $state:ident) => { ($delegate)($state).map_err(|error| trappable_error(error, TrappableError::downcast)) };
    (@result trap $result:ident) => { $result };
    (@result http $result:ident) => { $result.map_err(http_error) };
    (@result header $result:ident) => { $result.map_err(header_error) };
    (@result options $result:ident) => { $result.map_err(options_error) };
}

fn http_error(error: wasmtime::Error) -> HttpError {
    typed_error::<types::ErrorCode, _>(error, TrappableError::trap)
}
fn header_error(error: wasmtime::Error) -> HeaderError {
    typed_error::<types::HeaderError, _>(error, TrappableError::trap)
}
fn options_error(error: wasmtime::Error) -> OptionsError {
    typed_error::<types::RequestOptionsError, _>(error, TrappableError::trap)
}

fn delegate_access<'a, T>(store: &'a mut Access<'_, T, GateData<T>>) -> Access<'a, T, WasiHttp>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    Access::new(store.as_context_mut(), |state: &mut T| state.http())
}

impl<T> types::HostFields for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn new(&mut self) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "[constructor]fields", [], (), |state: &mut T| types::HostFields::new(&mut state.http()), |value: &Resource<FieldMap>| vec![value.rep()])
    }
    fn from_list(
        &mut self,
        entries: Vec<(types::FieldName, types::FieldValue)>,
    ) -> Result<Resource<FieldMap>, HeaderError> {
        call!(header self, "[static]fields.from-list", [], [entries = entries], |state: &mut T| types::HostFields::from_list(&mut state.http(), entries), |value: &Resource<FieldMap>| vec![value.rep()])
    }
    fn get(
        &mut self,
        fields: Resource<FieldMap>,
        name: types::FieldName,
    ) -> wasmtime::Result<Vec<types::FieldValue>> {
        call!(trap self, "[method]fields.get", [fields], [name = name], |state: &mut T| types::HostFields::get(&mut state.http(), fields, name))
    }
    fn has(
        &mut self,
        fields: Resource<FieldMap>,
        name: types::FieldName,
    ) -> wasmtime::Result<bool> {
        call!(trap self, "[method]fields.has", [fields], [name = name], |state: &mut T| types::HostFields::has(&mut state.http(), fields, name))
    }
    fn set(
        &mut self,
        fields: Resource<FieldMap>,
        name: types::FieldName,
        values: Vec<types::FieldValue>,
    ) -> Result<(), HeaderError> {
        call!(header self, "[method]fields.set", [fields], [name = name, values = values], |state: &mut T| types::HostFields::set(&mut state.http(), fields, name, values))
    }
    fn delete(
        &mut self,
        fields: Resource<FieldMap>,
        name: types::FieldName,
    ) -> Result<(), HeaderError> {
        call!(header self, "[method]fields.delete", [fields], [name = name], |state: &mut T| types::HostFields::delete(&mut state.http(), fields, name))
    }
    fn get_and_delete(
        &mut self,
        fields: Resource<FieldMap>,
        name: types::FieldName,
    ) -> Result<Vec<types::FieldValue>, HeaderError> {
        call!(header self, "[method]fields.get-and-delete", [fields], [name = name], |state: &mut T| types::HostFields::get_and_delete(&mut state.http(), fields, name))
    }
    fn append(
        &mut self,
        fields: Resource<FieldMap>,
        name: types::FieldName,
        value: types::FieldValue,
    ) -> Result<(), HeaderError> {
        call!(header self, "[method]fields.append", [fields], [name = name, value = value], |state: &mut T| types::HostFields::append(&mut state.http(), fields, name, value))
    }
    fn copy_all(
        &mut self,
        fields: Resource<FieldMap>,
    ) -> wasmtime::Result<Vec<(types::FieldName, types::FieldValue)>> {
        call!(trap self, "[method]fields.copy-all", [fields], (), |state: &mut T| types::HostFields::copy_all(&mut state.http(), fields))
    }
    fn clone(&mut self, fields: Resource<FieldMap>) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "[method]fields.clone", [fields], (), |state: &mut T| types::HostFields::clone(&mut state.http(), fields), |value: &Resource<FieldMap>| vec![value.rep()])
    }
    fn drop(&mut self, fields: Resource<FieldMap>) -> wasmtime::Result<()> {
        call!(trap self, "[resource-drop]fields", [fields], (), |state: &mut T| types::HostFields::drop(&mut state.http(), fields))
    }
}

impl<T> types::HostRequest for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn get_method(&mut self, request: Resource<Request>) -> wasmtime::Result<types::Method> {
        call!(trap self, "[method]request.get-method", [request], (), |state: &mut T| types::HostRequest::get_method(&mut state.http(), request))
    }
    fn set_method(
        &mut self,
        request: Resource<Request>,
        method: types::Method,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "[method]request.set-method", [request], [method = method], |state: &mut T| types::HostRequest::set_method(&mut state.http(), request, method))
    }
    fn get_path_with_query(
        &mut self,
        request: Resource<Request>,
    ) -> wasmtime::Result<Option<String>> {
        call!(trap self, "[method]request.get-path-with-query", [request], (), |state: &mut T| types::HostRequest::get_path_with_query(&mut state.http(), request))
    }
    fn set_path_with_query(
        &mut self,
        request: Resource<Request>,
        path: Option<String>,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "[method]request.set-path-with-query", [request], [path = path], |state: &mut T| types::HostRequest::set_path_with_query(&mut state.http(), request, path))
    }
    fn get_scheme(
        &mut self,
        request: Resource<Request>,
    ) -> wasmtime::Result<Option<types::Scheme>> {
        call!(trap self, "[method]request.get-scheme", [request], (), |state: &mut T| types::HostRequest::get_scheme(&mut state.http(), request))
    }
    fn set_scheme(
        &mut self,
        request: Resource<Request>,
        scheme: Option<types::Scheme>,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "[method]request.set-scheme", [request], [scheme = scheme], |state: &mut T| types::HostRequest::set_scheme(&mut state.http(), request, scheme))
    }
    fn get_authority(&mut self, request: Resource<Request>) -> wasmtime::Result<Option<String>> {
        call!(trap self, "[method]request.get-authority", [request], (), |state: &mut T| types::HostRequest::get_authority(&mut state.http(), request))
    }
    fn set_authority(
        &mut self,
        request: Resource<Request>,
        authority: Option<String>,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "[method]request.set-authority", [request], [authority = authority], |state: &mut T| types::HostRequest::set_authority(&mut state.http(), request, authority))
    }
    fn get_options(
        &mut self,
        request: Resource<Request>,
    ) -> wasmtime::Result<Option<Resource<types::RequestOptions>>> {
        call!(trap self, "[method]request.get-options", [request], (), |state: &mut T| types::HostRequest::get_options(&mut state.http(), request))
    }
    fn get_headers(&mut self, request: Resource<Request>) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "[method]request.get-headers", [request], (), |state: &mut T| types::HostRequest::get_headers(&mut state.http(), request), |value: &Resource<FieldMap>| vec![value.rep()])
    }
}

impl<T> types::HostRequestOptions for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn new(&mut self) -> wasmtime::Result<Resource<types::RequestOptions>> {
        call!(trap self, "[constructor]request-options", [], (), |state: &mut T| types::HostRequestOptions::new(&mut state.http()), |value: &Resource<types::RequestOptions>| vec![value.rep()])
    }
    fn get_connect_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
    ) -> wasmtime::Result<Option<Duration>> {
        call!(trap self, "[method]request-options.get-connect-timeout", [options], (), |state: &mut T| types::HostRequestOptions::get_connect_timeout(&mut state.http(), options))
    }
    fn set_connect_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
        duration: Option<Duration>,
    ) -> Result<(), OptionsError> {
        call!(options self, "[method]request-options.set-connect-timeout", [options], [duration = duration], |state: &mut T| types::HostRequestOptions::set_connect_timeout(&mut state.http(), options, duration))
    }
    fn get_first_byte_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
    ) -> wasmtime::Result<Option<Duration>> {
        call!(trap self, "[method]request-options.get-first-byte-timeout", [options], (), |state: &mut T| types::HostRequestOptions::get_first_byte_timeout(&mut state.http(), options))
    }
    fn set_first_byte_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
        duration: Option<Duration>,
    ) -> Result<(), OptionsError> {
        call!(options self, "[method]request-options.set-first-byte-timeout", [options], [duration = duration], |state: &mut T| types::HostRequestOptions::set_first_byte_timeout(&mut state.http(), options, duration))
    }
    fn get_between_bytes_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
    ) -> wasmtime::Result<Option<Duration>> {
        call!(trap self, "[method]request-options.get-between-bytes-timeout", [options], (), |state: &mut T| types::HostRequestOptions::get_between_bytes_timeout(&mut state.http(), options))
    }
    fn set_between_bytes_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
        duration: Option<Duration>,
    ) -> Result<(), OptionsError> {
        call!(options self, "[method]request-options.set-between-bytes-timeout", [options], [duration = duration], |state: &mut T| types::HostRequestOptions::set_between_bytes_timeout(&mut state.http(), options, duration))
    }
    fn clone(
        &mut self,
        options: Resource<types::RequestOptions>,
    ) -> wasmtime::Result<Resource<types::RequestOptions>> {
        call!(trap self, "[method]request-options.clone", [options], (), |state: &mut T| types::HostRequestOptions::clone(&mut state.http(), options), |value: &Resource<types::RequestOptions>| vec![value.rep()])
    }
    fn drop(&mut self, options: Resource<types::RequestOptions>) -> wasmtime::Result<()> {
        call!(trap self, "[resource-drop]request-options", [options], (), |state: &mut T| types::HostRequestOptions::drop(&mut state.http(), options))
    }
}

impl<T> types::HostResponse for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn get_status_code(
        &mut self,
        response: Resource<Response>,
    ) -> wasmtime::Result<types::StatusCode> {
        call!(trap self, "[method]response.get-status-code", [response], (), |state: &mut T| types::HostResponse::get_status_code(&mut state.http(), response))
    }
    fn set_status_code(
        &mut self,
        response: Resource<Response>,
        status: types::StatusCode,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "[method]response.set-status-code", [response], [status = status], |state: &mut T| types::HostResponse::set_status_code(&mut state.http(), response, status))
    }
    fn get_headers(
        &mut self,
        response: Resource<Response>,
    ) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "[method]response.get-headers", [response], (), |state: &mut T| types::HostResponse::get_headers(&mut state.http(), response), |value: &Resource<FieldMap>| vec![value.rep()])
    }
}

impl<T> types::Host for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn convert_error_code(&mut self, error: HttpError) -> wasmtime::Result<types::ErrorCode> {
        call!(trap self, "[error]error-code", [], [error = error], |state: &mut T| types::Host::convert_error_code(&mut state.http(), error))
    }
    fn convert_header_error(&mut self, error: HeaderError) -> wasmtime::Result<types::HeaderError> {
        call!(trap self, "[error]header-error", [], [error = error], |state: &mut T| types::Host::convert_header_error(&mut state.http(), error))
    }
    fn convert_request_options_error(
        &mut self,
        error: OptionsError,
    ) -> wasmtime::Result<types::RequestOptionsError> {
        call!(trap self, "[error]request-options-error", [], [error = error], |state: &mut T| types::Host::convert_request_options_error(&mut state.http(), error))
    }
}

impl<T> types::HostRequestWithStore<T> for GateData<T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn new(
        mut store: Access<T, Self>,
        headers: Resource<FieldMap>,
        contents: Option<StreamReader<u8>>,
        trailers: FutureReader<Result<Option<Resource<FieldMap>>, types::ErrorCode>>,
        options: Option<Resource<types::RequestOptions>>,
    ) -> wasmtime::Result<(
        Resource<Request>,
        FutureReader<Result<(), types::ErrorCode>>,
    )> {
        gate!(access store, VERSION, "wasi:http/types", "[static]request.new", handles = std::iter::once(headers.rep()).chain(options.as_ref().map(Resource::rep)), args = [options = options], delegate = |store| types::HostRequestWithStore::new(delegate_access(store), headers, contents, trailers, options), produced = |value: &(Resource<Request>, FutureReader<Result<(), types::ErrorCode>>)| vec![value.0.rep()])
    }
    fn consume_body(
        mut store: Access<T, Self>,
        request: Resource<Request>,
        result: FutureReader<Result<(), types::ErrorCode>>,
    ) -> wasmtime::Result<(
        StreamReader<u8>,
        FutureReader<Result<Option<Resource<FieldMap>>, types::ErrorCode>>,
    )> {
        gate!(access store, VERSION, "wasi:http/types", "[static]request.consume-body", handles = [request.rep()], args = (), delegate = |store| types::HostRequestWithStore::consume_body(delegate_access(store), request, result))
    }
    fn drop(mut store: Access<'_, T, Self>, request: Resource<Request>) -> wasmtime::Result<()> {
        gate!(access store, VERSION, "wasi:http/types", "[resource-drop]request", handles = [request.rep()], args = (), delegate = |store| types::HostRequestWithStore::drop(delegate_access(store), request))
    }
}

impl<T> types::HostResponseWithStore<T> for GateData<T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn new(
        mut store: Access<T, Self>,
        headers: Resource<FieldMap>,
        contents: Option<StreamReader<u8>>,
        trailers: FutureReader<Result<Option<Resource<FieldMap>>, types::ErrorCode>>,
    ) -> wasmtime::Result<(
        Resource<Response>,
        FutureReader<Result<(), types::ErrorCode>>,
    )> {
        gate!(access store, VERSION, "wasi:http/types", "[static]response.new", handles = [headers.rep()], args = (), delegate = |store| types::HostResponseWithStore::new(delegate_access(store), headers, contents, trailers), produced = |value: &(Resource<Response>, FutureReader<Result<(), types::ErrorCode>>)| vec![value.0.rep()])
    }
    fn consume_body(
        mut store: Access<T, Self>,
        response: Resource<Response>,
        result: FutureReader<Result<(), types::ErrorCode>>,
    ) -> wasmtime::Result<(
        StreamReader<u8>,
        FutureReader<Result<Option<Resource<FieldMap>>, types::ErrorCode>>,
    )> {
        gate!(access store, VERSION, "wasi:http/types", "[static]response.consume-body", handles = [response.rep()], args = (), delegate = |store| types::HostResponseWithStore::consume_body(delegate_access(store), response, result))
    }
    fn drop(mut store: Access<'_, T, Self>, response: Resource<Response>) -> wasmtime::Result<()> {
        gate!(access store, VERSION, "wasi:http/types", "[resource-drop]response", handles = [response.rep()], args = (), delegate = |store| types::HostResponseWithStore::drop(delegate_access(store), response))
    }
}

impl<T> client::Host for Gate<'_, T> where T: WasiHttpView + MiddlewareView + 'static {}

impl<T> client::HostWithStore<T> for GateData<T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    async fn send(
        store: &Accessor<T, Self>,
        request: Resource<Request>,
    ) -> Result<Resource<Response>, HttpError> {
        let delegate = store.with_getter::<WasiHttp>(|state: &mut T| state.http());
        let result = gate!(async store, VERSION, "wasi:http/client", "send", handles = [request.rep()], args = (), delegate = async { client::HostWithStore::send(&delegate, request).await.map_err(|error| trappable_error(error, TrappableError::downcast)) }, produced = |value: &Resource<Response>| vec![value.rep()]);
        result.map_err(
            |error| match error.downcast::<wasm_component_middleware::Denied>() {
                Ok(_) => types::ErrorCode::HttpRequestDenied.into(),
                Err(error) => http_error(error),
            },
        )
    }
}

/// Adds Preview 3 HTTP interfaces with middleware gates.
///
/// Component body streams are passed through unchanged.
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    client::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    types::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}
