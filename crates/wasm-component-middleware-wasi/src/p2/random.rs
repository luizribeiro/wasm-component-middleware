use wasm_component_middleware::MiddlewareView;
use wasmtime::component::Linker;
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p2::bindings::random::{insecure, insecure_seed, random};
use wasmtime_wasi::random::WasiRandomView;

use super::WASI_VERSION;
use super::gate::{Gate, GateData, gate, project};

impl<T> random::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_random_bytes(&mut self, len: u64) -> wasmtime::Result<Vec<u8>> {
        gate!(trap self, WASI_VERSION, "wasi:random/random", "get-random-bytes", handles = [], args = [len = len], delegate = |state: &mut T| random::Host::get_random_bytes(state.random(), len))
    }

    fn get_random_u64(&mut self) -> wasmtime::Result<u64> {
        gate!(trap self, WASI_VERSION, "wasi:random/random", "get-random-u64", handles = [], args = (), delegate = |state: &mut T| random::Host::get_random_u64(state.random()))
    }
}

impl<T> insecure::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_insecure_random_bytes(&mut self, len: u64) -> wasmtime::Result<Vec<u8>> {
        gate!(trap self, WASI_VERSION, "wasi:random/insecure", "get-insecure-random-bytes", handles = [], args = [len = len], delegate = |state: &mut T| insecure::Host::get_insecure_random_bytes(state.random(), len))
    }

    fn get_insecure_random_u64(&mut self) -> wasmtime::Result<u64> {
        gate!(trap self, WASI_VERSION, "wasi:random/insecure", "get-insecure-random-u64", handles = [], args = (), delegate = |state: &mut T| insecure::Host::get_insecure_random_u64(state.random()))
    }
}

impl<T> insecure_seed::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn insecure_seed(&mut self) -> wasmtime::Result<(u64, u64)> {
        gate!(trap self, WASI_VERSION, "wasi:random/insecure-seed", "insecure-seed", handles = [], args = (), delegate = |state: &mut T| insecure_seed::Host::insecure_seed(state.random()))
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    random::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    insecure::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    insecure_seed::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}
