use wasm_component_middleware::MiddlewareView;
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p2::SocketError;
use wasmtime_wasi::p2::bindings::sockets::ip_name_lookup::{
    self, HostResolveAddressStream, Pollable, ResolveAddressStream,
};
use wasmtime_wasi::p2::bindings::sockets::network::{IpAddress, Network};
use wasmtime_wasi::sockets::WasiSocketsView as _;

use super::super::WASI_VERSION;
use super::super::gate::{Gate, GateData, gate, produced_resource, project};

impl<T> ip_name_lookup::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn resolve_addresses(
        &mut self,
        network: Resource<Network>,
        name: String,
    ) -> Result<Resource<ResolveAddressStream>, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/ip-name-lookup", "resolve-addresses", handles = [network], args = [name = name.clone()], delegate = |state: &mut T| ip_name_lookup::Host::resolve_addresses(&mut state.sockets(), network, name), produced = produced_resource)
    }
}

impl<T> HostResolveAddressStream for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn resolve_next_address(
        &mut self,
        stream: Resource<ResolveAddressStream>,
    ) -> Result<Option<IpAddress>, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/ip-name-lookup", "[method]resolve-address-stream.resolve-next-address", handles = [stream], args = (), delegate = |state: &mut T| HostResolveAddressStream::resolve_next_address(&mut state.sockets(), stream))
    }

    fn subscribe(
        &mut self,
        stream: Resource<ResolveAddressStream>,
    ) -> wasmtime::Result<Resource<Pollable>> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/ip-name-lookup", "[method]resolve-address-stream.subscribe", handles = [stream], args = (), delegate = |state: &mut T| HostResolveAddressStream::subscribe(&mut state.sockets(), stream), produced = produced_resource)
    }

    async fn drop(&mut self, stream: Resource<ResolveAddressStream>) -> wasmtime::Result<()> {
        gate!(trap_state_async self, WASI_VERSION, "wasi:sockets/ip-name-lookup", "[resource-drop]resolve-address-stream", handles = [stream], args = (), delegate = async |state: &mut T| HostResolveAddressStream::drop(&mut state.sockets(), stream).await)
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    ip_name_lookup::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}
