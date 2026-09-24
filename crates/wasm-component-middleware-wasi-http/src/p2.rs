//! Middleware gates for WASI HTTP Preview 2 interfaces.

use wasm_component_middleware::{MiddlewareView, RoutedInterface};
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p2::{DynInputStream, DynOutputStream, DynPollable};
use wasmtime_wasi_http::p2::bindings::http::{outgoing_handler, types};
use wasmtime_wasi_http::p2::{HeaderError, HttpError};
use wasmtime_wasi_http::{FieldMap, WasiHttpView};

use crate::gate::{Gate, GateData, gate, project, trappable_error, typed_error};

const VERSION: &str = "0.2.12";

/// Interfaces routed by the Preview 2 linker functions.
pub const ROUTED_INTERFACES: &[RoutedInterface] = &[
    RoutedInterface::from_static("wasi:http/outgoing-handler"),
    RoutedInterface::from_static("wasi:http/types"),
];

macro_rules! call {
    ($kind:ident $self:ident, $interface:literal, $function:literal, [$($handle:expr),*], $args:tt, $delegate:expr $(, $produced:expr)?) => {{
        let result = gate!(trap $self, VERSION, $interface, $function, handles = [$($handle),*], args = $args, delegate = |state| call!(@delegate $kind $delegate, state) $(, produced = $produced)?);
        call!(@result $kind result)
    }};
    ($kind:ident $self:ident, $interface:literal, $function:literal, handles = $handles:expr, $args:tt, $delegate:expr $(, $produced:expr)?) => {{
        let result = gate!(trap_reps $self, VERSION, $interface, $function, handles = $handles, args = $args, delegate = |state| call!(@delegate $kind $delegate, state) $(, produced = $produced)?);
        call!(@result $kind result)
    }};
    (@delegate trap $delegate:expr, $state:ident) => { ($delegate)($state) };
    (@delegate http $delegate:expr, $state:ident) => {
        ($delegate)($state).map_err(|error| trappable_error(error, HttpError::downcast))
    };
    (@delegate header $delegate:expr, $state:ident) => {
        ($delegate)($state).map_err(|error| trappable_error(error, HeaderError::downcast))
    };
    (@result trap $result:ident) => { $result };
    (@result http $result:ident) => { $result.map_err(|error| typed_error::<types::ErrorCode, _>(error, HttpError::trap)) };
    (@result header $result:ident) => { $result.map_err(|error| typed_error::<types::HeaderError, _>(error, HeaderError::trap)) };
}

fn produced_result<T, E>(value: &Result<Resource<T>, E>) -> Vec<u32> {
    value.as_ref().ok().map(Resource::rep).into_iter().collect()
}

type FutureTrailersResult = Result<Result<Option<Resource<types::Trailers>>, types::ErrorCode>, ()>;

fn produced_future_trailers(value: Option<&FutureTrailersResult>) -> Vec<u32> {
    value
        .and_then(|value| value.as_ref().ok())
        .and_then(|value| value.as_ref().ok())
        .and_then(Option::as_ref)
        .map(Resource::rep)
        .into_iter()
        .collect()
}

type FutureResponseResult = Result<Result<Resource<types::IncomingResponse>, types::ErrorCode>, ()>;

fn produced_future_response(value: Option<&FutureResponseResult>) -> Vec<u32> {
    value
        .and_then(|value| value.as_ref().ok())
        .and_then(|value| value.as_ref().ok())
        .map(Resource::rep)
        .into_iter()
        .collect()
}

impl<T> outgoing_handler::Host for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn handle(
        &mut self,
        request: Resource<types::OutgoingRequest>,
        options: Option<Resource<types::RequestOptions>>,
    ) -> Result<Resource<types::FutureIncomingResponse>, HttpError> {
        call!(http self, "wasi:http/outgoing-handler", "handle", handles = std::iter::once(request.rep()).chain(options.as_ref().map(Resource::rep)), [options = options], |state: &mut T| outgoing_handler::Host::handle(&mut state.http(), request, options), |value: &Resource<types::FutureIncomingResponse>| vec![value.rep()])
    }
}

impl<T> types::Host for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn convert_error_code(&mut self, error: HttpError) -> wasmtime::Result<types::ErrorCode> {
        call!(trap self, "wasi:http/types", "[error]error-code", [], [error = error], |state: &mut T| types::Host::convert_error_code(&mut state.http(), error))
    }

    fn convert_header_error(&mut self, error: HeaderError) -> wasmtime::Result<types::HeaderError> {
        call!(trap self, "wasi:http/types", "[error]header-error", [], [error = error], |state: &mut T| types::Host::convert_header_error(&mut state.http(), error))
    }

    fn http_error_code(
        &mut self,
        error: Resource<types::IoError>,
    ) -> wasmtime::Result<Option<types::ErrorCode>> {
        call!(trap self, "wasi:http/types", "http-error-code", [error], (), |state: &mut T| types::Host::http_error_code(&mut state.http(), error))
    }
}

impl<T> types::HostFields for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn new(&mut self) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "wasi:http/types", "[constructor]fields", [], (), |state: &mut T| types::HostFields::new(&mut state.http()), |value: &Resource<FieldMap>| vec![value.rep()])
    }

    fn from_list(
        &mut self,
        entries: Vec<(String, Vec<u8>)>,
    ) -> Result<Resource<FieldMap>, HeaderError> {
        call!(header self, "wasi:http/types", "[static]fields.from-list", [], [entries = entries], |state: &mut T| types::HostFields::from_list(&mut state.http(), entries), |value: &Resource<FieldMap>| vec![value.rep()])
    }

    fn drop(&mut self, fields: Resource<FieldMap>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]fields", [fields], (), |state: &mut T| types::HostFields::drop(&mut state.http(), fields))
    }

    fn get(&mut self, fields: Resource<FieldMap>, name: String) -> wasmtime::Result<Vec<Vec<u8>>> {
        call!(trap self, "wasi:http/types", "[method]fields.get", [fields], [name = name], |state: &mut T| types::HostFields::get(&mut state.http(), fields, name))
    }

    fn has(&mut self, fields: Resource<FieldMap>, name: String) -> wasmtime::Result<bool> {
        call!(trap self, "wasi:http/types", "[method]fields.has", [fields], [name = name], |state: &mut T| types::HostFields::has(&mut state.http(), fields, name))
    }

    fn set(
        &mut self,
        fields: Resource<FieldMap>,
        name: String,
        values: Vec<Vec<u8>>,
    ) -> Result<(), HeaderError> {
        call!(header self, "wasi:http/types", "[method]fields.set", [fields], [name = name, values = values], |state: &mut T| types::HostFields::set(&mut state.http(), fields, name, values))
    }

    fn delete(&mut self, fields: Resource<FieldMap>, name: String) -> Result<(), HeaderError> {
        call!(header self, "wasi:http/types", "[method]fields.delete", [fields], [name = name], |state: &mut T| types::HostFields::delete(&mut state.http(), fields, name))
    }

    fn append(
        &mut self,
        fields: Resource<FieldMap>,
        name: String,
        value: Vec<u8>,
    ) -> Result<(), HeaderError> {
        call!(header self, "wasi:http/types", "[method]fields.append", [fields], [name = name, value = value], |state: &mut T| types::HostFields::append(&mut state.http(), fields, name, value))
    }

    fn entries(&mut self, fields: Resource<FieldMap>) -> wasmtime::Result<Vec<(String, Vec<u8>)>> {
        call!(trap self, "wasi:http/types", "[method]fields.entries", [fields], (), |state: &mut T| types::HostFields::entries(&mut state.http(), fields))
    }

    fn clone(&mut self, fields: Resource<FieldMap>) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "wasi:http/types", "[method]fields.clone", [fields], (), |state: &mut T| types::HostFields::clone(&mut state.http(), fields), |value: &Resource<FieldMap>| vec![value.rep()])
    }
}

impl<T> types::HostIncomingRequest for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn method(
        &mut self,
        request: Resource<types::IncomingRequest>,
    ) -> wasmtime::Result<types::Method> {
        call!(trap self, "wasi:http/types", "[method]incoming-request.method", [request], (), |state: &mut T| types::HostIncomingRequest::method(&mut state.http(), request))
    }
    fn path_with_query(
        &mut self,
        request: Resource<types::IncomingRequest>,
    ) -> wasmtime::Result<Option<String>> {
        call!(trap self, "wasi:http/types", "[method]incoming-request.path-with-query", [request], (), |state: &mut T| types::HostIncomingRequest::path_with_query(&mut state.http(), request))
    }
    fn scheme(
        &mut self,
        request: Resource<types::IncomingRequest>,
    ) -> wasmtime::Result<Option<types::Scheme>> {
        call!(trap self, "wasi:http/types", "[method]incoming-request.scheme", [request], (), |state: &mut T| types::HostIncomingRequest::scheme(&mut state.http(), request))
    }
    fn authority(
        &mut self,
        request: Resource<types::IncomingRequest>,
    ) -> wasmtime::Result<Option<String>> {
        call!(trap self, "wasi:http/types", "[method]incoming-request.authority", [request], (), |state: &mut T| types::HostIncomingRequest::authority(&mut state.http(), request))
    }
    fn headers(
        &mut self,
        request: Resource<types::IncomingRequest>,
    ) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "wasi:http/types", "[method]incoming-request.headers", [request], (), |state: &mut T| types::HostIncomingRequest::headers(&mut state.http(), request), |value: &Resource<FieldMap>| vec![value.rep()])
    }
    fn consume(
        &mut self,
        request: Resource<types::IncomingRequest>,
    ) -> wasmtime::Result<Result<Resource<types::IncomingBody>, ()>> {
        call!(trap self, "wasi:http/types", "[method]incoming-request.consume", [request], (), |state: &mut T| types::HostIncomingRequest::consume(&mut state.http(), request), produced_result)
    }
    fn drop(&mut self, request: Resource<types::IncomingRequest>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]incoming-request", [request], (), |state: &mut T| types::HostIncomingRequest::drop(&mut state.http(), request))
    }
}

impl<T> types::HostOutgoingRequest for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn new(
        &mut self,
        headers: Resource<FieldMap>,
    ) -> wasmtime::Result<Resource<types::OutgoingRequest>> {
        call!(trap self, "wasi:http/types", "[constructor]outgoing-request", [headers], (), |state: &mut T| types::HostOutgoingRequest::new(&mut state.http(), headers), |value: &Resource<types::OutgoingRequest>| vec![value.rep()])
    }
    fn body(
        &mut self,
        request: Resource<types::OutgoingRequest>,
    ) -> wasmtime::Result<Result<Resource<types::OutgoingBody>, ()>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.body", [request], (), |state: &mut T| types::HostOutgoingRequest::body(&mut state.http(), request), produced_result)
    }
    fn drop(&mut self, request: Resource<types::OutgoingRequest>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]outgoing-request", [request], (), |state: &mut T| types::HostOutgoingRequest::drop(&mut state.http(), request))
    }
    fn method(
        &mut self,
        request: Resource<types::OutgoingRequest>,
    ) -> wasmtime::Result<types::Method> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.method", [request], (), |state: &mut T| types::HostOutgoingRequest::method(&mut state.http(), request))
    }
    fn set_method(
        &mut self,
        request: Resource<types::OutgoingRequest>,
        method: types::Method,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.set-method", [request], [method = method], |state: &mut T| types::HostOutgoingRequest::set_method(&mut state.http(), request, method))
    }
    fn path_with_query(
        &mut self,
        request: Resource<types::OutgoingRequest>,
    ) -> wasmtime::Result<Option<String>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.path-with-query", [request], (), |state: &mut T| types::HostOutgoingRequest::path_with_query(&mut state.http(), request))
    }
    fn set_path_with_query(
        &mut self,
        request: Resource<types::OutgoingRequest>,
        path: Option<String>,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.set-path-with-query", [request], [path = path], |state: &mut T| types::HostOutgoingRequest::set_path_with_query(&mut state.http(), request, path))
    }
    fn scheme(
        &mut self,
        request: Resource<types::OutgoingRequest>,
    ) -> wasmtime::Result<Option<types::Scheme>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.scheme", [request], (), |state: &mut T| types::HostOutgoingRequest::scheme(&mut state.http(), request))
    }
    fn set_scheme(
        &mut self,
        request: Resource<types::OutgoingRequest>,
        scheme: Option<types::Scheme>,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.set-scheme", [request], [scheme = scheme], |state: &mut T| types::HostOutgoingRequest::set_scheme(&mut state.http(), request, scheme))
    }
    fn authority(
        &mut self,
        request: Resource<types::OutgoingRequest>,
    ) -> wasmtime::Result<Option<String>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.authority", [request], (), |state: &mut T| types::HostOutgoingRequest::authority(&mut state.http(), request))
    }
    fn set_authority(
        &mut self,
        request: Resource<types::OutgoingRequest>,
        authority: Option<String>,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.set-authority", [request], [authority = authority], |state: &mut T| types::HostOutgoingRequest::set_authority(&mut state.http(), request, authority))
    }
    fn headers(
        &mut self,
        request: Resource<types::OutgoingRequest>,
    ) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-request.headers", [request], (), |state: &mut T| types::HostOutgoingRequest::headers(&mut state.http(), request), |value: &Resource<FieldMap>| vec![value.rep()])
    }
}

impl<T> types::HostResponseOutparam for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn drop(&mut self, response: Resource<types::ResponseOutparam>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]response-outparam", [response], (), |state: &mut T| types::HostResponseOutparam::drop(&mut state.http(), response))
    }
    fn set(
        &mut self,
        response: Resource<types::ResponseOutparam>,
        value: Result<Resource<types::OutgoingResponse>, types::ErrorCode>,
    ) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[static]response-outparam.set", handles = std::iter::once(response.rep()).chain(value.as_ref().ok().map(Resource::rep)), [value = value], |state: &mut T| types::HostResponseOutparam::set(&mut state.http(), response, value))
    }
    fn send_informational(
        &mut self,
        response: Resource<types::ResponseOutparam>,
        status: u16,
        headers: Resource<FieldMap>,
    ) -> Result<(), HttpError> {
        call!(http self, "wasi:http/types", "[static]response-outparam.send-informational", [response, headers], [status = status], |state: &mut T| types::HostResponseOutparam::send_informational(&mut state.http(), response, status, headers))
    }
}

impl<T> types::HostIncomingResponse for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn drop(&mut self, response: Resource<types::IncomingResponse>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]incoming-response", [response], (), |state: &mut T| types::HostIncomingResponse::drop(&mut state.http(), response))
    }
    fn status(
        &mut self,
        response: Resource<types::IncomingResponse>,
    ) -> wasmtime::Result<types::StatusCode> {
        call!(trap self, "wasi:http/types", "[method]incoming-response.status", [response], (), |state: &mut T| types::HostIncomingResponse::status(&mut state.http(), response))
    }
    fn headers(
        &mut self,
        response: Resource<types::IncomingResponse>,
    ) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "wasi:http/types", "[method]incoming-response.headers", [response], (), |state: &mut T| types::HostIncomingResponse::headers(&mut state.http(), response), |value: &Resource<FieldMap>| vec![value.rep()])
    }
    fn consume(
        &mut self,
        response: Resource<types::IncomingResponse>,
    ) -> wasmtime::Result<Result<Resource<types::IncomingBody>, ()>> {
        call!(trap self, "wasi:http/types", "[method]incoming-response.consume", [response], (), |state: &mut T| types::HostIncomingResponse::consume(&mut state.http(), response), produced_result)
    }
}

impl<T> types::HostFutureTrailers for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn drop(&mut self, trailers: Resource<types::FutureTrailers>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]future-trailers", [trailers], (), |state: &mut T| types::HostFutureTrailers::drop(&mut state.http(), trailers))
    }
    fn subscribe(
        &mut self,
        trailers: Resource<types::FutureTrailers>,
    ) -> wasmtime::Result<Resource<DynPollable>> {
        call!(trap self, "wasi:http/types", "[method]future-trailers.subscribe", [trailers], (), |state: &mut T| types::HostFutureTrailers::subscribe(&mut state.http(), trailers), |value: &Resource<DynPollable>| vec![value.rep()])
    }
    fn get(
        &mut self,
        trailers: Resource<types::FutureTrailers>,
    ) -> wasmtime::Result<
        Option<Result<Result<Option<Resource<types::Trailers>>, types::ErrorCode>, ()>>,
    > {
        call!(trap self, "wasi:http/types", "[method]future-trailers.get", [trailers], (), |state: &mut T| types::HostFutureTrailers::get(&mut state.http(), trailers), |value: &Option<FutureTrailersResult>| produced_future_trailers(value.as_ref()))
    }
}

impl<T> types::HostIncomingBody for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn stream(
        &mut self,
        body: Resource<types::IncomingBody>,
    ) -> wasmtime::Result<Result<Resource<DynInputStream>, ()>> {
        call!(trap self, "wasi:http/types", "[method]incoming-body.stream", [body], (), |state: &mut T| types::HostIncomingBody::stream(&mut state.http(), body), produced_result)
    }
    fn finish(
        &mut self,
        body: Resource<types::IncomingBody>,
    ) -> wasmtime::Result<Resource<types::FutureTrailers>> {
        call!(trap self, "wasi:http/types", "[static]incoming-body.finish", [body], (), |state: &mut T| types::HostIncomingBody::finish(&mut state.http(), body), |value: &Resource<types::FutureTrailers>| vec![value.rep()])
    }
    fn drop(&mut self, body: Resource<types::IncomingBody>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]incoming-body", [body], (), |state: &mut T| types::HostIncomingBody::drop(&mut state.http(), body))
    }
}

impl<T> types::HostOutgoingResponse for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn new(
        &mut self,
        headers: Resource<FieldMap>,
    ) -> wasmtime::Result<Resource<types::OutgoingResponse>> {
        call!(trap self, "wasi:http/types", "[constructor]outgoing-response", [headers], (), |state: &mut T| types::HostOutgoingResponse::new(&mut state.http(), headers), |value: &Resource<types::OutgoingResponse>| vec![value.rep()])
    }
    fn body(
        &mut self,
        response: Resource<types::OutgoingResponse>,
    ) -> wasmtime::Result<Result<Resource<types::OutgoingBody>, ()>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-response.body", [response], (), |state: &mut T| types::HostOutgoingResponse::body(&mut state.http(), response), produced_result)
    }
    fn status_code(
        &mut self,
        response: Resource<types::OutgoingResponse>,
    ) -> wasmtime::Result<types::StatusCode> {
        call!(trap self, "wasi:http/types", "[method]outgoing-response.status-code", [response], (), |state: &mut T| types::HostOutgoingResponse::status_code(&mut state.http(), response))
    }
    fn set_status_code(
        &mut self,
        response: Resource<types::OutgoingResponse>,
        status: types::StatusCode,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-response.set-status-code", [response], [status = status], |state: &mut T| types::HostOutgoingResponse::set_status_code(&mut state.http(), response, status))
    }
    fn headers(
        &mut self,
        response: Resource<types::OutgoingResponse>,
    ) -> wasmtime::Result<Resource<FieldMap>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-response.headers", [response], (), |state: &mut T| types::HostOutgoingResponse::headers(&mut state.http(), response), |value: &Resource<FieldMap>| vec![value.rep()])
    }
    fn drop(&mut self, response: Resource<types::OutgoingResponse>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]outgoing-response", [response], (), |state: &mut T| types::HostOutgoingResponse::drop(&mut state.http(), response))
    }
}

impl<T> types::HostFutureIncomingResponse for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn drop(&mut self, response: Resource<types::FutureIncomingResponse>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]future-incoming-response", [response], (), |state: &mut T| types::HostFutureIncomingResponse::drop(&mut state.http(), response))
    }
    fn get(
        &mut self,
        response: Resource<types::FutureIncomingResponse>,
    ) -> wasmtime::Result<
        Option<Result<Result<Resource<types::IncomingResponse>, types::ErrorCode>, ()>>,
    > {
        call!(trap self, "wasi:http/types", "[method]future-incoming-response.get", [response], (), |state: &mut T| types::HostFutureIncomingResponse::get(&mut state.http(), response), |value: &Option<FutureResponseResult>| produced_future_response(value.as_ref()))
    }
    fn subscribe(
        &mut self,
        response: Resource<types::FutureIncomingResponse>,
    ) -> wasmtime::Result<Resource<DynPollable>> {
        call!(trap self, "wasi:http/types", "[method]future-incoming-response.subscribe", [response], (), |state: &mut T| types::HostFutureIncomingResponse::subscribe(&mut state.http(), response), |value: &Resource<DynPollable>| vec![value.rep()])
    }
}

impl<T> types::HostOutgoingBody for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn write(
        &mut self,
        body: Resource<types::OutgoingBody>,
    ) -> wasmtime::Result<Result<Resource<DynOutputStream>, ()>> {
        call!(trap self, "wasi:http/types", "[method]outgoing-body.write", [body], (), |state: &mut T| types::HostOutgoingBody::write(&mut state.http(), body), produced_result)
    }
    fn finish(
        &mut self,
        body: Resource<types::OutgoingBody>,
        trailers: Option<Resource<types::Trailers>>,
    ) -> Result<(), HttpError> {
        call!(http self, "wasi:http/types", "[static]outgoing-body.finish", handles = std::iter::once(body.rep()).chain(trailers.as_ref().map(Resource::rep)), [trailers = trailers], |state: &mut T| types::HostOutgoingBody::finish(&mut state.http(), body, trailers))
    }
    fn drop(&mut self, body: Resource<types::OutgoingBody>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]outgoing-body", [body], (), |state: &mut T| types::HostOutgoingBody::drop(&mut state.http(), body))
    }
}

impl<T> types::HostRequestOptions for Gate<'_, T>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    fn new(&mut self) -> wasmtime::Result<Resource<types::RequestOptions>> {
        call!(trap self, "wasi:http/types", "[constructor]request-options", [], (), |state: &mut T| types::HostRequestOptions::new(&mut state.http()), |value: &Resource<types::RequestOptions>| vec![value.rep()])
    }
    fn connect_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
    ) -> wasmtime::Result<Option<types::Duration>> {
        call!(trap self, "wasi:http/types", "[method]request-options.connect-timeout", [options], (), |state: &mut T| types::HostRequestOptions::connect_timeout(&mut state.http(), options))
    }
    fn set_connect_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
        duration: Option<types::Duration>,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "wasi:http/types", "[method]request-options.set-connect-timeout", [options], [duration = duration], |state: &mut T| types::HostRequestOptions::set_connect_timeout(&mut state.http(), options, duration))
    }
    fn first_byte_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
    ) -> wasmtime::Result<Option<types::Duration>> {
        call!(trap self, "wasi:http/types", "[method]request-options.first-byte-timeout", [options], (), |state: &mut T| types::HostRequestOptions::first_byte_timeout(&mut state.http(), options))
    }
    fn set_first_byte_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
        duration: Option<types::Duration>,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "wasi:http/types", "[method]request-options.set-first-byte-timeout", [options], [duration = duration], |state: &mut T| types::HostRequestOptions::set_first_byte_timeout(&mut state.http(), options, duration))
    }
    fn between_bytes_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
    ) -> wasmtime::Result<Option<types::Duration>> {
        call!(trap self, "wasi:http/types", "[method]request-options.between-bytes-timeout", [options], (), |state: &mut T| types::HostRequestOptions::between_bytes_timeout(&mut state.http(), options))
    }
    fn set_between_bytes_timeout(
        &mut self,
        options: Resource<types::RequestOptions>,
        duration: Option<types::Duration>,
    ) -> wasmtime::Result<Result<(), ()>> {
        call!(trap self, "wasi:http/types", "[method]request-options.set-between-bytes-timeout", [options], [duration = duration], |state: &mut T| types::HostRequestOptions::set_between_bytes_timeout(&mut state.http(), options, duration))
    }
    fn drop(&mut self, options: Resource<types::RequestOptions>) -> wasmtime::Result<()> {
        call!(trap self, "wasi:http/types", "[resource-drop]request-options", [options], (), |state: &mut T| types::HostRequestOptions::drop(&mut state.http(), options))
    }
}

fn add_http_async<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    let options = wasmtime_wasi_http::p2::bindings::LinkOptions::default();
    outgoing_handler::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    types::add_to_linker::<T, GateData<T>>(linker, &options.into(), project::<T>)
}

fn add_http_sync<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    let options = wasmtime_wasi_http::p2::bindings::LinkOptions::default();
    wasmtime_wasi_http::p2::bindings::sync::http::outgoing_handler::add_to_linker::<T, GateData<T>>(
        linker,
        project::<T>,
    )?;
    wasmtime_wasi_http::p2::bindings::sync::http::types::add_to_linker::<T, GateData<T>>(
        linker,
        &options.into(),
        project::<T>,
    )
}

/// Adds asynchronous Preview 2 HTTP interfaces with middleware gates.
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_only_http_to_linker_async<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    add_http_async(linker)
}

/// Adds synchronous Preview 2 HTTP interfaces with middleware gates.
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_only_http_to_linker_sync<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiHttpView + MiddlewareView + 'static,
{
    add_http_sync(linker)
}

/// Adds asynchronous Preview 2 proxy and HTTP interfaces with middleware gates.
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker_async<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiHttpView + WasiView + MiddlewareView + 'static,
{
    wasm_component_middleware_wasi::p2::add_to_linker_proxy_interfaces_async(linker)?;
    add_http_async(linker)
}

/// Adds synchronous Preview 2 WASI proxy and HTTP interfaces with middleware gates.
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker_sync<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiHttpView + WasiView + MiddlewareView + 'static,
{
    wasm_component_middleware_wasi::p2::add_to_linker_sync(linker)?;
    add_http_sync(linker)
}
