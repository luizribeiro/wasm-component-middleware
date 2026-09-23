use wasm_component_middleware::MiddlewareView;
use wasmtime::component::{Accessor, Linker};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p3::bindings::sockets::{ip_name_lookup, types};
use wasmtime_wasi::sockets::{WasiSockets, WasiSocketsView as _};

use crate::gate::{Gate, GateData, gate, project};

impl<T> ip_name_lookup::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> ip_name_lookup::HostWithStore<T> for GateData<T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn resolve_addresses(
        store: &Accessor<T, Self>,
        name: String,
    ) -> wasmtime::Result<Result<Vec<types::IpAddress>, ip_name_lookup::ErrorCode>> {
        let delegate = store.with_getter::<WasiSockets>(|state: &mut T| state.sockets());
        match gate!(async store, "wasi:sockets/ip-name-lookup", "resolve-addresses", handles = [], args = [name = name.clone()], delegate = ip_name_lookup::HostWithStore::resolve_addresses(&delegate, name))
        {
            Ok(result) => Ok(result),
            Err(error) => match error.downcast::<wasm_component_middleware::Denied>() {
                Ok(_) => Ok(Err(ip_name_lookup::ErrorCode::AccessDenied)),
                Err(error) => Err(error),
            },
        }
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    ip_name_lookup::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}
