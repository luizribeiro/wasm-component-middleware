use wasm_component_middleware::{ArgumentValue, MiddlewareView};
use wasmtime::component::{Accessor, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p3::bindings::sockets::types::{
    HostUdpSocket, HostUdpSocketWithStore, IpAddressFamily, IpSocketAddress, UdpSocket,
};
use wasmtime_wasi::p3::sockets::SocketResult;
use wasmtime_wasi::sockets::{WasiSockets, WasiSocketsView as _};

use crate::gate::{Gate, GateData, gate, produced_resource};

use super::super::WASI_VERSION;
use super::{address, optional_address};

const INTERFACE: &str = "wasi:sockets/types";

impl<T> HostUdpSocket for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn bind(
        &mut self,
        socket: Resource<UdpSocket>,
        local_address: IpSocketAddress,
    ) -> SocketResult<()> {
        gate!(socket3_state_async self, WASI_VERSION, INTERFACE, "[method]udp-socket.bind", handles = [socket], args = [local_address = address(local_address)], delegate = async |state: &mut T| {
            let mut sockets = state.sockets();
            HostUdpSocket::bind(&mut sockets, socket, local_address).await
        })
    }

    async fn connect(
        &mut self,
        socket: Resource<UdpSocket>,
        remote_address: IpSocketAddress,
    ) -> SocketResult<()> {
        gate!(socket3_state_async self, WASI_VERSION, INTERFACE, "[method]udp-socket.connect", handles = [socket], args = [remote_address = address(remote_address)], delegate = async |state: &mut T| {
            let mut sockets = state.sockets();
            HostUdpSocket::connect(&mut sockets, socket, remote_address).await
        })
    }

    async fn create(
        &mut self,
        address_family: IpAddressFamily,
    ) -> SocketResult<Resource<UdpSocket>> {
        gate!(socket3_state_async self, WASI_VERSION, INTERFACE, "[static]udp-socket.create", handles = [], args = (address_family), delegate = async |state: &mut T| {
            let mut sockets = state.sockets();
            HostUdpSocket::create(&mut sockets, address_family).await
        }, produced = produced_resource)
    }

    fn disconnect(&mut self, socket: Resource<UdpSocket>) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]udp-socket.disconnect", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::disconnect(&mut state.sockets(), socket))
    }

    fn get_local_address(&mut self, socket: Resource<UdpSocket>) -> SocketResult<IpSocketAddress> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]udp-socket.get-local-address", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::get_local_address(&mut state.sockets(), socket))
    }

    fn get_remote_address(&mut self, socket: Resource<UdpSocket>) -> SocketResult<IpSocketAddress> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]udp-socket.get-remote-address", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::get_remote_address(&mut state.sockets(), socket))
    }

    fn get_address_family(
        &mut self,
        socket: Resource<UdpSocket>,
    ) -> wasmtime::Result<IpAddressFamily> {
        gate!(trap self, WASI_VERSION, INTERFACE, "[method]udp-socket.get-address-family", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::get_address_family(&mut state.sockets(), socket))
    }

    fn get_unicast_hop_limit(&mut self, socket: Resource<UdpSocket>) -> SocketResult<u8> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]udp-socket.get-unicast-hop-limit", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::get_unicast_hop_limit(&mut state.sockets(), socket))
    }

    fn set_unicast_hop_limit(
        &mut self,
        socket: Resource<UdpSocket>,
        value: u8,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]udp-socket.set-unicast-hop-limit", handles = [socket], args = [value = value], delegate = |state: &mut T| HostUdpSocket::set_unicast_hop_limit(&mut state.sockets(), socket, value))
    }

    fn get_receive_buffer_size(&mut self, socket: Resource<UdpSocket>) -> SocketResult<u64> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]udp-socket.get-receive-buffer-size", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::get_receive_buffer_size(&mut state.sockets(), socket))
    }

    fn set_receive_buffer_size(
        &mut self,
        socket: Resource<UdpSocket>,
        value: u64,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]udp-socket.set-receive-buffer-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostUdpSocket::set_receive_buffer_size(&mut state.sockets(), socket, value))
    }

    fn get_send_buffer_size(&mut self, socket: Resource<UdpSocket>) -> SocketResult<u64> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]udp-socket.get-send-buffer-size", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::get_send_buffer_size(&mut state.sockets(), socket))
    }

    fn set_send_buffer_size(
        &mut self,
        socket: Resource<UdpSocket>,
        value: u64,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]udp-socket.set-send-buffer-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostUdpSocket::set_send_buffer_size(&mut state.sockets(), socket, value))
    }

    fn drop(&mut self, socket: Resource<UdpSocket>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, INTERFACE, "[resource-drop]udp-socket", handles = [socket], args = (), delegate = |state: &mut T| HostUdpSocket::drop(&mut state.sockets(), socket))
    }
}

impl<T, M> HostUdpSocketWithStore<T> for GateData<T, M>
where
    T: WasiView + MiddlewareView + 'static,
    M: 'static,
{
    async fn send(
        store: &Accessor<T, Self>,
        socket: Resource<UdpSocket>,
        data: Vec<u8>,
        remote_address: Option<IpSocketAddress>,
    ) -> SocketResult<()> {
        let delegate = store.with_getter::<WasiSockets>(|state: &mut T| state.sockets());
        gate!(socket3_async store, INTERFACE, "[method]udp-socket.send", handles = [socket.rep()], args = [data = ArgumentValue::bytes(&data), remote_address = optional_address(remote_address)], delegate = HostUdpSocketWithStore::send(&delegate, socket, data, remote_address))
    }

    async fn receive(
        store: &Accessor<T, Self>,
        socket: Resource<UdpSocket>,
    ) -> SocketResult<(Vec<u8>, IpSocketAddress)> {
        let delegate = store.with_getter::<WasiSockets>(|state: &mut T| state.sockets());
        gate!(socket3_async store, INTERFACE, "[method]udp-socket.receive", handles = [socket.rep()], args = (), delegate = HostUdpSocketWithStore::receive(&delegate, socket))
    }
}
