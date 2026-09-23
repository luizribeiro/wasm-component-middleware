use wasm_component_middleware::MiddlewareView;
use wasmtime::component::Linker;
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p3::bindings::random::{insecure, insecure_seed, random};
use wasmtime_wasi::random::WasiRandomView;

use crate::gate::{Gate, GateData, gate, project};

use super::WASI_VERSION;

impl<T> random::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_random_bytes(&mut self, max_len: u64) -> wasmtime::Result<Vec<u8>> {
        gate!(trap self, WASI_VERSION, "wasi:random/random", "get-random-bytes", handles = [], args = [max_len = max_len], delegate = |state: &mut T| random::Host::get_random_bytes(state.random(), max_len))
    }

    fn get_random_u64(&mut self) -> wasmtime::Result<u64> {
        gate!(trap self, WASI_VERSION, "wasi:random/random", "get-random-u64", handles = [], args = (), delegate = |state: &mut T| random::Host::get_random_u64(state.random()))
    }
}

impl<T> insecure::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_insecure_random_bytes(&mut self, max_len: u64) -> wasmtime::Result<Vec<u8>> {
        gate!(trap self, WASI_VERSION, "wasi:random/insecure", "get-insecure-random-bytes", handles = [], args = [max_len = max_len], delegate = |state: &mut T| insecure::Host::get_insecure_random_bytes(state.random(), max_len))
    }

    fn get_insecure_random_u64(&mut self) -> wasmtime::Result<u64> {
        gate!(trap self, WASI_VERSION, "wasi:random/insecure", "get-insecure-random-u64", handles = [], args = (), delegate = |state: &mut T| insecure::Host::get_insecure_random_u64(state.random()))
    }
}

impl<T> insecure_seed::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn get_insecure_seed(&mut self) -> wasmtime::Result<(u64, u64)> {
        gate!(trap self, WASI_VERSION, "wasi:random/insecure-seed", "get-insecure-seed", handles = [], args = (), delegate = |state: &mut T| insecure_seed::Host::get_insecure_seed(state.random()))
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
