use wasm_component_middleware::MiddlewareView;
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::p2::SocketError;
use wasmtime_wasi::p2::bindings::sockets::{instance_network, network, tcp_create_socket};
use wasmtime_wasi::p2::bindings::sync::LinkOptions;
use wasmtime_wasi::sockets::WasiSocketsView as _;
use wasmtime_wasi::{WasiView, p2::TcpSocket};

use super::super::WASI_VERSION;
use super::super::gate::{Gate, GateData, gate, produced_resource, project};

impl<T> network::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn convert_error_code(&mut self, error: SocketError) -> wasmtime::Result<network::ErrorCode> {
        network::Host::convert_error_code(&mut self.state.sockets(), error)
    }

    fn network_error_code(
        &mut self,
        error: Resource<wasmtime::Error>,
    ) -> wasmtime::Result<Option<network::ErrorCode>> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/network", "network-error-code", handles = [error], args = (), delegate = |state: &mut T| network::Host::network_error_code(&mut state.sockets(), error))
    }
}

impl<T> network::HostNetwork for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn drop(&mut self, network: Resource<network::Network>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/network", "[resource-drop]network", handles = [network], args = (), delegate = |state: &mut T| network::HostNetwork::drop(&mut state.sockets(), network))
    }
}

impl<T> instance_network::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn instance_network(&mut self) -> wasmtime::Result<Resource<network::Network>> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/instance-network", "instance-network", handles = [], args = (), delegate = |state: &mut T| instance_network::Host::instance_network(&mut state.sockets()), produced = produced_resource)
    }
}

impl<T> tcp_create_socket::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn create_tcp_socket(
        &mut self,
        address_family: network::IpAddressFamily,
    ) -> Result<Resource<TcpSocket>, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp-create-socket", "create-tcp-socket", handles = [], args = (address_family), delegate = |state: &mut T| tcp_create_socket::Host::create_tcp_socket(&mut state.sockets(), address_family), produced = produced_resource)
    }
}

pub(crate) fn add_to_linker<T>(
    linker: &mut Linker<T>,
    options: &LinkOptions,
) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    tcp_create_socket::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    instance_network::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    network::add_to_linker::<T, GateData<T>>(linker, &options.into(), project::<T>)
}
