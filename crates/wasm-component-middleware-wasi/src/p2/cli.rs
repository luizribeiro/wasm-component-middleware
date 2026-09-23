use wasm_component_middleware::MiddlewareView;
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::cli::WasiCliView;
use wasmtime_wasi::p2::bindings::cli::{
    environment, exit, stderr, stdin, stdout, terminal_input, terminal_output, terminal_stderr,
    terminal_stdin, terminal_stdout,
};
use wasmtime_wasi::p2::{DynInputStream, DynOutputStream};

use super::gate::{Gate, GateData, gate, project};

impl<T> environment::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_environment(&mut self) -> wasmtime::Result<Vec<(String, String)>> {
        gate!(trap self, "wasi:cli/environment", "get-environment", handles = [], args = (), delegate = |state: &mut T| environment::Host::get_environment(&mut state.cli()))
    }

    fn get_arguments(&mut self) -> wasmtime::Result<Vec<String>> {
        gate!(trap self, "wasi:cli/environment", "get-arguments", handles = [], args = (), delegate = |state: &mut T| environment::Host::get_arguments(&mut state.cli()))
    }

    fn initial_cwd(&mut self) -> wasmtime::Result<Option<String>> {
        gate!(trap self, "wasi:cli/environment", "initial-cwd", handles = [], args = (), delegate = |state: &mut T| environment::Host::initial_cwd(&mut state.cli()))
    }
}

impl<T> exit::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn exit(&mut self, status: Result<(), ()>) -> wasmtime::Result<()> {
        gate!(trap self, "wasi:cli/exit", "exit", handles = [], args = [status = wasm_component_middleware::ArgumentValue::case(if status.is_ok() { "ok" } else { "err" })], delegate = |state: &mut T| exit::Host::exit(&mut state.cli(), status))
    }

    fn exit_with_code(&mut self, status_code: u8) -> wasmtime::Result<()> {
        gate!(trap self, "wasi:cli/exit", "exit-with-code", handles = [], args = [status_code = status_code], delegate = |state: &mut T| exit::Host::exit_with_code(&mut state.cli(), status_code))
    }
}

impl<T> stdin::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_stdin(&mut self) -> wasmtime::Result<Resource<DynInputStream>> {
        gate!(trap self, "wasi:cli/stdin", "get-stdin", handles = [], args = (), delegate = |state: &mut T| stdin::Host::get_stdin(&mut state.cli()), produced = |value: &Resource<DynInputStream>| vec![value.rep()])
    }
}

impl<T> stdout::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_stdout(&mut self) -> wasmtime::Result<Resource<DynOutputStream>> {
        gate!(trap self, "wasi:cli/stdout", "get-stdout", handles = [], args = (), delegate = |state: &mut T| stdout::Host::get_stdout(&mut state.cli()), produced = |value: &Resource<DynOutputStream>| vec![value.rep()])
    }
}

impl<T> stderr::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_stderr(&mut self) -> wasmtime::Result<Resource<DynOutputStream>> {
        gate!(trap self, "wasi:cli/stderr", "get-stderr", handles = [], args = (), delegate = |state: &mut T| stderr::Host::get_stderr(&mut state.cli()), produced = |value: &Resource<DynOutputStream>| vec![value.rep()])
    }
}

impl<T> terminal_input::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> terminal_input::HostTerminalInput for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn drop(&mut self, terminal: Resource<terminal_input::TerminalInput>) -> wasmtime::Result<()> {
        gate!(trap self, "wasi:cli/terminal-input", "[resource-drop]terminal-input", handles = [terminal], args = (), delegate = |state: &mut T| terminal_input::HostTerminalInput::drop(&mut state.cli(), terminal))
    }
}

impl<T> terminal_output::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> terminal_output::HostTerminalOutput for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn drop(
        &mut self,
        terminal: Resource<terminal_output::TerminalOutput>,
    ) -> wasmtime::Result<()> {
        gate!(trap self, "wasi:cli/terminal-output", "[resource-drop]terminal-output", handles = [terminal], args = (), delegate = |state: &mut T| terminal_output::HostTerminalOutput::drop(&mut state.cli(), terminal))
    }
}

impl<T> terminal_stdin::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_terminal_stdin(
        &mut self,
    ) -> wasmtime::Result<Option<Resource<terminal_input::TerminalInput>>> {
        gate!(trap self, "wasi:cli/terminal-stdin", "get-terminal-stdin", handles = [], args = (), delegate = |state: &mut T| terminal_stdin::Host::get_terminal_stdin(&mut state.cli()), produced = |value: &Option<Resource<terminal_input::TerminalInput>>| value.as_ref().map(Resource::rep).into_iter().collect())
    }
}

impl<T> terminal_stdout::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_terminal_stdout(
        &mut self,
    ) -> wasmtime::Result<Option<Resource<terminal_output::TerminalOutput>>> {
        gate!(trap self, "wasi:cli/terminal-stdout", "get-terminal-stdout", handles = [], args = (), delegate = |state: &mut T| terminal_stdout::Host::get_terminal_stdout(&mut state.cli()), produced = |value: &Option<Resource<terminal_output::TerminalOutput>>| value.as_ref().map(Resource::rep).into_iter().collect())
    }
}

impl<T> terminal_stderr::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_terminal_stderr(
        &mut self,
    ) -> wasmtime::Result<Option<Resource<terminal_output::TerminalOutput>>> {
        gate!(trap self, "wasi:cli/terminal-stderr", "get-terminal-stderr", handles = [], args = (), delegate = |state: &mut T| terminal_stderr::Host::get_terminal_stderr(&mut state.cli()), produced = |value: &Option<Resource<terminal_output::TerminalOutput>>| value.as_ref().map(Resource::rep).into_iter().collect())
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
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
