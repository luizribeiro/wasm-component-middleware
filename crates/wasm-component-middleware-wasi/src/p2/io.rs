use wasm_component_middleware::MiddlewareView;
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p2::bindings::sync::io::{error, poll, streams};
use wasmtime_wasi::p2::{DynPollable, IoError, StreamError, StreamResult};

use super::gate::{Gate, GateData, gate, project};

impl<T> error::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> error::HostError for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn drop(&mut self, error: Resource<IoError>) -> wasmtime::Result<()> {
        gate!(trap self, "wasi:io/error", "[resource-drop]error", handles = [error], args = (), delegate = |state: &mut T| error::HostError::drop(state.ctx().table, error))
    }

    fn to_debug_string(&mut self, error: Resource<IoError>) -> wasmtime::Result<String> {
        gate!(trap self, "wasi:io/error", "[method]error.to-debug-string", handles = [error], args = (), delegate = |state: &mut T| error::HostError::to_debug_string(state.ctx().table, error))
    }
}

impl<T> poll::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn poll(&mut self, pollables: Vec<Resource<DynPollable>>) -> wasmtime::Result<Vec<u32>> {
        gate!(trap_each self, "wasi:io/poll", "poll", handles = pollables, args = (), delegate = |state: &mut T| poll::Host::poll(state.ctx().table, pollables))
    }
}

impl<T> poll::HostPollable for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn ready(&mut self, pollable: Resource<DynPollable>) -> wasmtime::Result<bool> {
        gate!(trap self, "wasi:io/poll", "[method]pollable.ready", handles = [pollable], args = (), delegate = |state: &mut T| poll::HostPollable::ready(state.ctx().table, pollable))
    }

    fn block(&mut self, pollable: Resource<DynPollable>) -> wasmtime::Result<()> {
        gate!(trap self, "wasi:io/poll", "[method]pollable.block", handles = [pollable], args = (), delegate = |state: &mut T| poll::HostPollable::block(state.ctx().table, pollable))
    }

    fn drop(&mut self, pollable: Resource<DynPollable>) -> wasmtime::Result<()> {
        gate!(trap self, "wasi:io/poll", "[resource-drop]pollable", handles = [pollable], args = (), delegate = |state: &mut T| poll::HostPollable::drop(state.ctx().table, pollable))
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
    fn drop(&mut self, stream: Resource<streams::InputStream>) -> wasmtime::Result<()> {
        gate!(trap self, "wasi:io/streams", "[resource-drop]input-stream", handles = [stream], args = (), delegate = |state: &mut T| streams::HostInputStream::drop(state.ctx().table, stream))
    }

    fn read(&mut self, stream: Resource<streams::InputStream>, len: u64) -> StreamResult<Vec<u8>> {
        gate!(stream self, "wasi:io/streams", "[method]input-stream.read", handles = [stream], args = (len,), delegate = |state: &mut T| streams::HostInputStream::read(state.ctx().table, stream, len))
    }

    fn blocking_read(
        &mut self,
        stream: Resource<streams::InputStream>,
        len: u64,
    ) -> StreamResult<Vec<u8>> {
        gate!(stream self, "wasi:io/streams", "[method]input-stream.blocking-read", handles = [stream], args = (len,), delegate = |state: &mut T| streams::HostInputStream::blocking_read(state.ctx().table, stream, len))
    }

    fn skip(&mut self, stream: Resource<streams::InputStream>, len: u64) -> StreamResult<u64> {
        gate!(stream self, "wasi:io/streams", "[method]input-stream.skip", handles = [stream], args = (len,), delegate = |state: &mut T| streams::HostInputStream::skip(state.ctx().table, stream, len))
    }

    fn blocking_skip(
        &mut self,
        stream: Resource<streams::InputStream>,
        len: u64,
    ) -> StreamResult<u64> {
        gate!(stream self, "wasi:io/streams", "[method]input-stream.blocking-skip", handles = [stream], args = (len,), delegate = |state: &mut T| streams::HostInputStream::blocking_skip(state.ctx().table, stream, len))
    }

    fn subscribe(
        &mut self,
        stream: Resource<streams::InputStream>,
    ) -> wasmtime::Result<Resource<DynPollable>> {
        gate!(trap self, "wasi:io/streams", "[method]input-stream.subscribe", handles = [stream], args = (), delegate = |state: &mut T| streams::HostInputStream::subscribe(state.ctx().table, stream), produced = |value: &Resource<DynPollable>| vec![value.rep()])
    }
}

impl<T> streams::HostOutputStream for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn drop(&mut self, stream: Resource<streams::OutputStream>) -> wasmtime::Result<()> {
        gate!(trap self, "wasi:io/streams", "[resource-drop]output-stream", handles = [stream], args = (), delegate = |state: &mut T| streams::HostOutputStream::drop(state.ctx().table, stream))
    }

    fn check_write(&mut self, stream: Resource<streams::OutputStream>) -> StreamResult<u64> {
        gate!(stream self, "wasi:io/streams", "[method]output-stream.check-write", handles = [stream], args = (), delegate = |state: &mut T| streams::HostOutputStream::check_write(state.ctx().table, stream))
    }

    fn write(
        &mut self,
        stream: Resource<streams::OutputStream>,
        bytes: Vec<u8>,
    ) -> StreamResult<()> {
        gate!(stream self, "wasi:io/streams", "[method]output-stream.write", handles = [stream], args = (bytes.clone(),), delegate = |state: &mut T| streams::HostOutputStream::write(state.ctx().table, stream, bytes))
    }

    fn blocking_write_and_flush(
        &mut self,
        stream: Resource<streams::OutputStream>,
        bytes: Vec<u8>,
    ) -> StreamResult<()> {
        gate!(stream self, "wasi:io/streams", "[method]output-stream.blocking-write-and-flush", handles = [stream], args = (bytes.clone(),), delegate = |state: &mut T| streams::HostOutputStream::blocking_write_and_flush(state.ctx().table, stream, bytes))
    }

    fn blocking_write_zeroes_and_flush(
        &mut self,
        stream: Resource<streams::OutputStream>,
        len: u64,
    ) -> StreamResult<()> {
        gate!(stream self, "wasi:io/streams", "[method]output-stream.blocking-write-zeroes-and-flush", handles = [stream], args = (len,), delegate = |state: &mut T| streams::HostOutputStream::blocking_write_zeroes_and_flush(state.ctx().table, stream, len))
    }

    fn subscribe(
        &mut self,
        stream: Resource<streams::OutputStream>,
    ) -> wasmtime::Result<Resource<DynPollable>> {
        gate!(trap self, "wasi:io/streams", "[method]output-stream.subscribe", handles = [stream], args = (), delegate = |state: &mut T| streams::HostOutputStream::subscribe(state.ctx().table, stream), produced = |value: &Resource<DynPollable>| vec![value.rep()])
    }

    fn write_zeroes(
        &mut self,
        stream: Resource<streams::OutputStream>,
        len: u64,
    ) -> StreamResult<()> {
        gate!(stream self, "wasi:io/streams", "[method]output-stream.write-zeroes", handles = [stream], args = (len,), delegate = |state: &mut T| streams::HostOutputStream::write_zeroes(state.ctx().table, stream, len))
    }

    fn flush(&mut self, stream: Resource<streams::OutputStream>) -> StreamResult<()> {
        gate!(stream self, "wasi:io/streams", "[method]output-stream.flush", handles = [stream], args = (), delegate = |state: &mut T| streams::HostOutputStream::flush(state.ctx().table, stream))
    }

    fn blocking_flush(&mut self, stream: Resource<streams::OutputStream>) -> StreamResult<()> {
        gate!(stream self, "wasi:io/streams", "[method]output-stream.blocking-flush", handles = [stream], args = (), delegate = |state: &mut T| streams::HostOutputStream::blocking_flush(state.ctx().table, stream))
    }

    fn splice(
        &mut self,
        destination: Resource<streams::OutputStream>,
        source: Resource<streams::InputStream>,
        len: u64,
    ) -> StreamResult<u64> {
        gate!(stream self, "wasi:io/streams", "[method]output-stream.splice", handles = [destination, source], args = (len,), delegate = |state: &mut T| streams::HostOutputStream::splice(state.ctx().table, destination, source, len))
    }

    fn blocking_splice(
        &mut self,
        destination: Resource<streams::OutputStream>,
        source: Resource<streams::InputStream>,
        len: u64,
    ) -> StreamResult<u64> {
        gate!(stream self, "wasi:io/streams", "[method]output-stream.blocking-splice", handles = [destination, source], args = (len,), delegate = |state: &mut T| streams::HostOutputStream::blocking_splice(state.ctx().table, destination, source, len))
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
