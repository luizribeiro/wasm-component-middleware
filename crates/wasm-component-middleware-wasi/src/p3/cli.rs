use wasm_component_middleware::MiddlewareView;
use wasmtime::AsContextMut;
use wasmtime::component::{Access, FutureReader, Linker, Resource, StreamReader};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::cli::{WasiCli, WasiCliView};
use wasmtime_wasi::p3::bindings::cli::{
    environment, exit, stderr, stdin, stdout, terminal_input, terminal_output, terminal_stderr,
    terminal_stdin, terminal_stdout, types,
};
use wasmtime_wasi::p3::cli::{TerminalInput, TerminalOutput};

use crate::gate::{Gate, GateData, gate, project};

use super::WASI_VERSION;

fn delegate_access<'a, T>(store: &'a mut Access<'_, T, GateData<T>>) -> Access<'a, T, WasiCli>
where
    T: WasiView + MiddlewareView + 'static,
{
    Access::new(store.as_context_mut(), |state: &mut T| state.cli())
}

impl<T> types::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> environment::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_environment(&mut self) -> wasmtime::Result<Vec<(String, String)>> {
        gate!(trap self, WASI_VERSION, "wasi:cli/environment", "get-environment", handles = [], args = (), delegate = |state: &mut T| environment::Host::get_environment(&mut state.cli()))
    }

    fn get_arguments(&mut self) -> wasmtime::Result<Vec<String>> {
        gate!(trap self, WASI_VERSION, "wasi:cli/environment", "get-arguments", handles = [], args = (), delegate = |state: &mut T| environment::Host::get_arguments(&mut state.cli()))
    }

    fn get_initial_cwd(&mut self) -> wasmtime::Result<Option<String>> {
        gate!(trap self, WASI_VERSION, "wasi:cli/environment", "get-initial-cwd", handles = [], args = (), delegate = |state: &mut T| environment::Host::get_initial_cwd(&mut state.cli()))
    }
}

impl<T> exit::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn exit(&mut self, status: Result<(), ()>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:cli/exit", "exit", handles = [], args = [status = wasm_component_middleware::ArgumentValue::case(if status.is_ok() { "ok" } else { "err" })], delegate = |state: &mut T| exit::Host::exit(&mut state.cli(), status))
    }

    fn exit_with_code(&mut self, status_code: u8) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:cli/exit", "exit-with-code", handles = [], args = [status_code = status_code], delegate = |state: &mut T| exit::Host::exit_with_code(&mut state.cli(), status_code))
    }
}

impl<T> stdin::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> stdin::HostWithStore<T> for GateData<T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn read_via_stream(
        mut store: Access<T, Self>,
    ) -> wasmtime::Result<(StreamReader<u8>, FutureReader<Result<(), types::ErrorCode>>)> {
        gate!(access store, "wasi:cli/stdin", "read-via-stream", handles = [], args = (), delegate = |store| stdin::HostWithStore::read_via_stream(delegate_access(store)))
    }
}

impl<T> stdout::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> stdout::HostWithStore<T> for GateData<T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn write_via_stream(
        mut store: Access<'_, T, Self>,
        data: StreamReader<u8>,
    ) -> wasmtime::Result<FutureReader<Result<(), types::ErrorCode>>> {
        gate!(access store, "wasi:cli/stdout", "write-via-stream", handles = [], args = (), delegate = |store| stdout::HostWithStore::write_via_stream(delegate_access(store), data))
    }
}

impl<T> stderr::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> stderr::HostWithStore<T> for GateData<T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn write_via_stream(
        mut store: Access<'_, T, Self>,
        data: StreamReader<u8>,
    ) -> wasmtime::Result<FutureReader<Result<(), types::ErrorCode>>> {
        gate!(access store, "wasi:cli/stderr", "write-via-stream", handles = [], args = (), delegate = |store| stderr::HostWithStore::write_via_stream(delegate_access(store), data))
    }
}

impl<T> terminal_input::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> terminal_input::HostTerminalInput for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn drop(&mut self, terminal: Resource<TerminalInput>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:cli/terminal-input", "[resource-drop]terminal-input", handles = [terminal], args = (), delegate = |state: &mut T| terminal_input::HostTerminalInput::drop(&mut state.cli(), terminal))
    }
}

impl<T> terminal_output::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> terminal_output::HostTerminalOutput for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn drop(&mut self, terminal: Resource<TerminalOutput>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:cli/terminal-output", "[resource-drop]terminal-output", handles = [terminal], args = (), delegate = |state: &mut T| terminal_output::HostTerminalOutput::drop(&mut state.cli(), terminal))
    }
}

impl<T> terminal_stdin::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_terminal_stdin(&mut self) -> wasmtime::Result<Option<Resource<TerminalInput>>> {
        gate!(trap self, WASI_VERSION, "wasi:cli/terminal-stdin", "get-terminal-stdin", handles = [], args = (), delegate = |state: &mut T| terminal_stdin::Host::get_terminal_stdin(&mut state.cli()), produced = |value: &Option<Resource<TerminalInput>>| value.as_ref().map(Resource::rep).into_iter().collect())
    }
}

impl<T> terminal_stdout::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_terminal_stdout(&mut self) -> wasmtime::Result<Option<Resource<TerminalOutput>>> {
        gate!(trap self, WASI_VERSION, "wasi:cli/terminal-stdout", "get-terminal-stdout", handles = [], args = (), delegate = |state: &mut T| terminal_stdout::Host::get_terminal_stdout(&mut state.cli()), produced = |value: &Option<Resource<TerminalOutput>>| value.as_ref().map(Resource::rep).into_iter().collect())
    }
}

impl<T> terminal_stderr::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_terminal_stderr(&mut self) -> wasmtime::Result<Option<Resource<TerminalOutput>>> {
        gate!(trap self, WASI_VERSION, "wasi:cli/terminal-stderr", "get-terminal-stderr", handles = [], args = (), delegate = |state: &mut T| terminal_stderr::Host::get_terminal_stderr(&mut state.cli()), produced = |value: &Option<Resource<TerminalOutput>>| value.as_ref().map(Resource::rep).into_iter().collect())
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    types::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    environment::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    exit::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    stdin::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    stdout::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    stderr::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    terminal_input::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    terminal_output::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    terminal_stdin::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    terminal_stdout::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    terminal_stderr::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}
