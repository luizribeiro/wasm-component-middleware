use wasm_component_middleware::MiddlewareView;
use wasmtime::component::{Accessor, Linker};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::clocks::{WasiClocks, WasiClocksView};
use wasmtime_wasi::p3::bindings::clocks::{monotonic_clock, system_clock, types};

use crate::gate::{Gate, GateData, gate, project};

use super::WASI_VERSION;

impl<T> types::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> system_clock::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn now(&mut self) -> wasmtime::Result<system_clock::Instant> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/system-clock", "now", handles = [], args = (), delegate = |state: &mut T| system_clock::Host::now(&mut state.clocks()))
    }

    fn get_resolution(&mut self) -> wasmtime::Result<types::Duration> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/system-clock", "get-resolution", handles = [], args = (), delegate = |state: &mut T| system_clock::Host::get_resolution(&mut state.clocks()))
    }
}

impl<T> monotonic_clock::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn now(&mut self) -> wasmtime::Result<monotonic_clock::Mark> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/monotonic-clock", "now", handles = [], args = (), delegate = |state: &mut T| monotonic_clock::Host::now(&mut state.clocks()))
    }

    fn get_resolution(&mut self) -> wasmtime::Result<types::Duration> {
        gate!(trap self, WASI_VERSION, "wasi:clocks/monotonic-clock", "get-resolution", handles = [], args = (), delegate = |state: &mut T| monotonic_clock::Host::get_resolution(&mut state.clocks()))
    }
}

impl<T> monotonic_clock::HostWithStore<T> for GateData<T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn wait_until(
        store: &Accessor<T, Self>,
        when: monotonic_clock::Mark,
    ) -> wasmtime::Result<()> {
        let delegate = store.with_getter::<WasiClocks>(|state: &mut T| state.clocks());
        gate!(async store, "wasi:clocks/monotonic-clock", "wait-until", handles = [], args = [when = when], delegate = monotonic_clock::HostWithStore::wait_until(&delegate, when))
    }

    async fn wait_for(
        store: &Accessor<T, Self>,
        duration: types::Duration,
    ) -> wasmtime::Result<()> {
        let delegate = store.with_getter::<WasiClocks>(|state: &mut T| state.clocks());
        gate!(async store, "wasi:clocks/monotonic-clock", "wait-for", handles = [], args = [duration = duration], delegate = monotonic_clock::HostWithStore::wait_for(&delegate, duration))
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    types::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    monotonic_clock::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    system_clock::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}
