use wasm_component_middleware::MiddlewareView;
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p2::SocketError;
use wasmtime_wasi::p2::bindings::sockets::network::Network;
use wasmtime_wasi::p2::bindings::sockets::udp::{
    self, HostIncomingDatagramStream, HostOutgoingDatagramStream, HostUdpSocket, IncomingDatagram,
    IncomingDatagramStream, IpAddressFamily, IpSocketAddress, OutgoingDatagram,
    OutgoingDatagramStream, Pollable, UdpSocket,
};
use wasmtime_wasi::p2::bindings::sockets::udp_create_socket;
use wasmtime_wasi::sockets::WasiSocketsView as _;

use super::super::WASI_VERSION;
use super::super::gate::{Gate, GateData, gate, produced_resource, project};
use super::{address, async_datagrams, optional_address};

impl<T> udp::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> HostUdpSocket for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn start_bind(
        &mut self,
        socket: Resource<UdpSocket>,
        network: Resource<Network>,
        local_address: IpSocketAddress,
    ) -> Result<(), SocketError> {
        gate!(socket_state_async self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.start-bind", handles = [socket, network], args = [local_address = address(local_address)], delegate = async |state: &mut T| HostUdpSocket::start_bind(&mut state.sockets(), socket, network, local_address).await)
    }

    fn finish_bind(&mut self, socket: Resource<UdpSocket>) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.finish-bind", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::finish_bind(&mut state.sockets(), socket))
    }

    async fn stream(
        &mut self,
        socket: Resource<UdpSocket>,
        remote_address: Option<IpSocketAddress>,
    ) -> Result<
        (
            Resource<IncomingDatagramStream>,
            Resource<OutgoingDatagramStream>,
        ),
        SocketError,
    > {
        gate!(socket_state_async self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.stream", handles = [socket], args = [remote_address = optional_address(remote_address)], delegate = async |state: &mut T| HostUdpSocket::stream(&mut state.sockets(), socket, remote_address).await, produced = |value: &(Resource<IncomingDatagramStream>, Resource<OutgoingDatagramStream>)| vec![value.0.rep(), value.1.rep()])
    }

    fn local_address(
        &mut self,
        socket: Resource<UdpSocket>,
    ) -> Result<IpSocketAddress, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.local-address", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::local_address(&mut state.sockets(), socket))
    }

    fn remote_address(
        &mut self,
        socket: Resource<UdpSocket>,
    ) -> Result<IpSocketAddress, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.remote-address", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::remote_address(&mut state.sockets(), socket))
    }

    fn address_family(&mut self, socket: Resource<UdpSocket>) -> wasmtime::Result<IpAddressFamily> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.address-family", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::address_family(&mut state.sockets(), socket))
    }

    fn unicast_hop_limit(&mut self, socket: Resource<UdpSocket>) -> Result<u8, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.unicast-hop-limit", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::unicast_hop_limit(&mut state.sockets(), socket))
    }

    fn set_unicast_hop_limit(
        &mut self,
        socket: Resource<UdpSocket>,
        value: u8,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.set-unicast-hop-limit", handles = [socket], args = [value = value], delegate = |state: &mut T| HostUdpSocket::set_unicast_hop_limit(&mut state.sockets(), socket, value))
    }

    fn receive_buffer_size(&mut self, socket: Resource<UdpSocket>) -> Result<u64, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.receive-buffer-size", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::receive_buffer_size(&mut state.sockets(), socket))
    }

    fn set_receive_buffer_size(
        &mut self,
        socket: Resource<UdpSocket>,
        value: u64,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.set-receive-buffer-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostUdpSocket::set_receive_buffer_size(&mut state.sockets(), socket, value))
    }

    fn send_buffer_size(&mut self, socket: Resource<UdpSocket>) -> Result<u64, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.send-buffer-size", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::send_buffer_size(&mut state.sockets(), socket))
    }

    fn set_send_buffer_size(
        &mut self,
        socket: Resource<UdpSocket>,
        value: u64,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.set-send-buffer-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostUdpSocket::set_send_buffer_size(&mut state.sockets(), socket, value))
    }

    fn subscribe(&mut self, socket: Resource<UdpSocket>) -> wasmtime::Result<Resource<Pollable>> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/udp", "[method]udp-socket.subscribe", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::subscribe(&mut state.sockets(), socket), produced = produced_resource)
    }

    fn drop(&mut self, socket: Resource<UdpSocket>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/udp", "[resource-drop]udp-socket", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::drop(&mut state.sockets(), socket))
    }
}

impl<T> HostIncomingDatagramStream for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn receive(
        &mut self,
        stream: Resource<IncomingDatagramStream>,
        max_results: u64,
    ) -> Result<Vec<IncomingDatagram>, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]incoming-datagram-stream.receive", handles = [stream], args = [max_results = max_results], delegate = |state: &mut T| HostIncomingDatagramStream::receive(&mut state.sockets(), stream, max_results))
    }

    fn subscribe(
        &mut self,
        stream: Resource<IncomingDatagramStream>,
    ) -> wasmtime::Result<Resource<Pollable>> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/udp", "[method]incoming-datagram-stream.subscribe", handles = [stream], args = (), delegate = |state: &mut T| HostIncomingDatagramStream::subscribe(&mut state.sockets(), stream), produced = produced_resource)
    }

    async fn drop(&mut self, stream: Resource<IncomingDatagramStream>) -> wasmtime::Result<()> {
        gate!(trap_state_async self, WASI_VERSION, "wasi:sockets/udp", "[resource-drop]incoming-datagram-stream", handles = [stream], args = (), delegate = async |state: &mut T| HostIncomingDatagramStream::drop(&mut state.sockets(), stream).await)
    }
}

impl<T> HostOutgoingDatagramStream for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn check_send(&mut self, stream: Resource<OutgoingDatagramStream>) -> Result<u64, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]outgoing-datagram-stream.check-send", handles = [stream], args = (), delegate = |state: &mut T| HostOutgoingDatagramStream::check_send(&mut state.sockets(), stream))
    }

    fn send(
        &mut self,
        stream: Resource<OutgoingDatagramStream>,
        values: Vec<OutgoingDatagram>,
    ) -> Result<u64, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/udp", "[method]outgoing-datagram-stream.send", handles = [stream], args = [datagrams = async_datagrams(&values)], delegate = |state: &mut T| HostOutgoingDatagramStream::send(&mut state.sockets(), stream, values))
    }

    fn subscribe(
        &mut self,
        stream: Resource<OutgoingDatagramStream>,
    ) -> wasmtime::Result<Resource<Pollable>> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/udp", "[method]outgoing-datagram-stream.subscribe", handles = [stream], args = (), delegate = |state: &mut T| HostOutgoingDatagramStream::subscribe(&mut state.sockets(), stream), produced = produced_resource)
    }

    async fn drop(&mut self, stream: Resource<OutgoingDatagramStream>) -> wasmtime::Result<()> {
        gate!(trap_state_async self, WASI_VERSION, "wasi:sockets/udp", "[resource-drop]outgoing-datagram-stream", handles = [stream], args = (), delegate = async |state: &mut T| HostOutgoingDatagramStream::drop(&mut state.sockets(), stream).await)
    }
}

impl<T> udp_create_socket::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn create_udp_socket(
        &mut self,
        address_family: IpAddressFamily,
    ) -> Result<Resource<UdpSocket>, SocketError> {
        gate!(socket_state_async self, WASI_VERSION, "wasi:sockets/udp-create-socket", "create-udp-socket", handles = [], args = (address_family), delegate = async |state: &mut T| udp_create_socket::Host::create_udp_socket(&mut state.sockets(), address_family).await, produced = produced_resource)
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    udp::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    udp_create_socket::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}
