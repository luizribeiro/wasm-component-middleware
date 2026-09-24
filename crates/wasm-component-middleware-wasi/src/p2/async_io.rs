use wasm_component_middleware::MiddlewareView;
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p2::bindings::io::{error, poll, streams};
use wasmtime_wasi::p2::{DynPollable, StreamError, StreamResult};

use super::WASI_VERSION;
use super::gate::{Gate, GateData, gate, project};

impl<T> poll::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn poll(&mut self, pollables: Vec<Resource<DynPollable>>) -> wasmtime::Result<Vec<u32>> {
        gate!(trap_each_state_async self, WASI_VERSION, "wasi:io/poll", "poll", handles = pollables, args = (), delegate = async |state: &mut T| poll::Host::poll(state.ctx().table, pollables).await)
    }
}

impl<T> poll::HostPollable for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn ready(&mut self, pollable: Resource<DynPollable>) -> wasmtime::Result<bool> {
        gate!(trap_state_async self, WASI_VERSION, "wasi:io/poll", "[method]pollable.ready", handles = [pollable], args = (), delegate = async |state: &mut T| poll::HostPollable::ready(state.ctx().table, pollable).await)
    }

    async fn block(&mut self, pollable: Resource<DynPollable>) -> wasmtime::Result<()> {
        gate!(trap_state_async self, WASI_VERSION, "wasi:io/poll", "[method]pollable.block", handles = [pollable], args = (), delegate = async |state: &mut T| poll::HostPollable::block(state.ctx().table, pollable).await)
    }

    fn drop(&mut self, pollable: Resource<DynPollable>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:io/poll", "[resource-drop]pollable", handles = [pollable], args = (), delegate = |state: &mut T| poll::HostPollable::drop(state.ctx().table, pollable))
    }
}

impl<T> streams::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn convert_stream_error(
        &mut self,
        error: StreamError,
    ) -> wasmtime::Result<streams::StreamError> {
        streams::Host::convert_stream_error(self.state.ctx().table, error)
    }
}

impl<T> streams::HostInputStream for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn drop(&mut self, stream: Resource<streams::InputStream>) -> wasmtime::Result<()> {
        gate!(trap_state_async self, WASI_VERSION, "wasi:io/streams", "[resource-drop]input-stream", handles = [stream], args = (), delegate = async |state: &mut T| streams::HostInputStream::drop(state.ctx().table, stream).await)
    }

    fn read(&mut self, stream: Resource<streams::InputStream>, len: u64) -> StreamResult<Vec<u8>> {
        gate!(stream self, WASI_VERSION, "wasi:io/streams", "[method]input-stream.read", handles = [stream], args = [len = len], delegate = |state: &mut T| streams::HostInputStream::read(state.ctx().table, stream, len))
    }

    async fn blocking_read(
        &mut self,
        stream: Resource<streams::InputStream>,
        len: u64,
    ) -> StreamResult<Vec<u8>> {
        gate!(stream_state_async self, WASI_VERSION, "wasi:io/streams", "[method]input-stream.blocking-read", handles = [stream], args = [len = len], delegate = async |state: &mut T| streams::HostInputStream::blocking_read(state.ctx().table, stream, len).await)
    }

    fn skip(&mut self, stream: Resource<streams::InputStream>, len: u64) -> StreamResult<u64> {
        gate!(stream self, WASI_VERSION, "wasi:io/streams", "[method]input-stream.skip", handles = [stream], args = [len = len], delegate = |state: &mut T| streams::HostInputStream::skip(state.ctx().table, stream, len))
    }

    async fn blocking_skip(
        &mut self,
        stream: Resource<streams::InputStream>,
        len: u64,
    ) -> StreamResult<u64> {
        gate!(stream_state_async self, WASI_VERSION, "wasi:io/streams", "[method]input-stream.blocking-skip", handles = [stream], args = [len = len], delegate = async |state: &mut T| streams::HostInputStream::blocking_skip(state.ctx().table, stream, len).await)
    }

    fn subscribe(
        &mut self,
        stream: Resource<streams::InputStream>,
    ) -> wasmtime::Result<Resource<DynPollable>> {
        gate!(trap self, WASI_VERSION, "wasi:io/streams", "[method]input-stream.subscribe", handles = [stream], args = (), delegate = |state: &mut T| streams::HostInputStream::subscribe(state.ctx().table, stream), produced = |value: &Resource<DynPollable>| vec![value.rep()])
    }
}

impl<T> streams::HostOutputStream for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn drop(&mut self, stream: Resource<streams::OutputStream>) -> wasmtime::Result<()> {
        gate!(trap_state_async self, WASI_VERSION, "wasi:io/streams", "[resource-drop]output-stream", handles = [stream], args = (), delegate = async |state: &mut T| streams::HostOutputStream::drop(state.ctx().table, stream).await)
    }

    fn check_write(&mut self, stream: Resource<streams::OutputStream>) -> StreamResult<u64> {
        gate!(stream self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.check-write", handles = [stream], args = (), delegate = |state: &mut T| streams::HostOutputStream::check_write(state.ctx().table, stream))
    }

    fn write(
        &mut self,
        stream: Resource<streams::OutputStream>,
        bytes: Vec<u8>,
    ) -> StreamResult<()> {
        gate!(stream self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.write", handles = [stream], args = [bytes = wasm_component_middleware::ArgumentValue::bytes(&bytes)], delegate = |state: &mut T| streams::HostOutputStream::write(state.ctx().table, stream, bytes))
    }

    async fn blocking_write_and_flush(
        &mut self,
        stream: Resource<streams::OutputStream>,
        bytes: Vec<u8>,
    ) -> StreamResult<()> {
        gate!(stream_state_async self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.blocking-write-and-flush", handles = [stream], args = [bytes = wasm_component_middleware::ArgumentValue::bytes(&bytes)], delegate = async |state: &mut T| streams::HostOutputStream::blocking_write_and_flush(state.ctx().table, stream, bytes).await)
    }

    async fn blocking_write_zeroes_and_flush(
        &mut self,
        stream: Resource<streams::OutputStream>,
        len: u64,
    ) -> StreamResult<()> {
        gate!(stream_state_async self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.blocking-write-zeroes-and-flush", handles = [stream], args = [len = len], delegate = async |state: &mut T| streams::HostOutputStream::blocking_write_zeroes_and_flush(state.ctx().table, stream, len).await)
    }

    fn subscribe(
        &mut self,
        stream: Resource<streams::OutputStream>,
    ) -> wasmtime::Result<Resource<DynPollable>> {
        gate!(trap self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.subscribe", handles = [stream], args = (), delegate = |state: &mut T| streams::HostOutputStream::subscribe(state.ctx().table, stream), produced = |value: &Resource<DynPollable>| vec![value.rep()])
    }

    fn write_zeroes(
        &mut self,
        stream: Resource<streams::OutputStream>,
        len: u64,
    ) -> StreamResult<()> {
        gate!(stream self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.write-zeroes", handles = [stream], args = [len = len], delegate = |state: &mut T| streams::HostOutputStream::write_zeroes(state.ctx().table, stream, len))
    }

    fn flush(&mut self, stream: Resource<streams::OutputStream>) -> StreamResult<()> {
        gate!(stream self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.flush", handles = [stream], args = (), delegate = |state: &mut T| streams::HostOutputStream::flush(state.ctx().table, stream))
    }

    async fn blocking_flush(
        &mut self,
        stream: Resource<streams::OutputStream>,
    ) -> StreamResult<()> {
        gate!(stream_state_async self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.blocking-flush", handles = [stream], args = (), delegate = async |state: &mut T| streams::HostOutputStream::blocking_flush(state.ctx().table, stream).await)
    }

    fn splice(
        &mut self,
        destination: Resource<streams::OutputStream>,
        source: Resource<streams::InputStream>,
        len: u64,
    ) -> StreamResult<u64> {
        gate!(stream self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.splice", handles = [destination, source], args = [len = len], delegate = |state: &mut T| streams::HostOutputStream::splice(state.ctx().table, destination, source, len))
    }

    async fn blocking_splice(
        &mut self,
        destination: Resource<streams::OutputStream>,
        source: Resource<streams::InputStream>,
        len: u64,
    ) -> StreamResult<u64> {
        gate!(stream_state_async self, WASI_VERSION, "wasi:io/streams", "[method]output-stream.blocking-splice", handles = [destination, source], args = [len = len], delegate = async |state: &mut T| streams::HostOutputStream::blocking_splice(state.ctx().table, destination, source, len).await)
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    error::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    poll::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    streams::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}
