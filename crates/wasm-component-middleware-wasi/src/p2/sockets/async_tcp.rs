use wasm_component_middleware::{ArgumentValue, MiddlewareView};
use wasmtime::component::{Linker, Resource};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p2::SocketError;
use wasmtime_wasi::p2::bindings::sockets::network::Network;
use wasmtime_wasi::p2::bindings::sockets::tcp::{
    self, Duration, HostTcpSocket, InputStream, IpAddressFamily, IpSocketAddress, OutputStream,
    Pollable, ShutdownType, TcpSocket,
};
use wasmtime_wasi::sockets::WasiSocketsView as _;

use super::super::WASI_VERSION;
use super::super::gate::{Gate, GateData, gate, produced_resource, project};
use super::address;

impl<T> tcp::Host for Gate<'_, T> where T: WasiView + MiddlewareView + 'static {}

impl<T> HostTcpSocket for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn start_bind(
        &mut self,
        socket: Resource<TcpSocket>,
        network: Resource<Network>,
        local_address: IpSocketAddress,
    ) -> Result<(), SocketError> {
        gate!(socket_state_async self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.start-bind", handles = [socket, network], args = [local_address = address(local_address)], delegate = async |state: &mut T| HostTcpSocket::start_bind(&mut state.sockets(), socket, network, local_address).await)
    }

    fn finish_bind(&mut self, socket: Resource<TcpSocket>) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.finish-bind", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::finish_bind(&mut state.sockets(), socket))
    }

    fn start_connect(
        &mut self,
        socket: Resource<TcpSocket>,
        network: Resource<Network>,
        remote_address: IpSocketAddress,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.start-connect", handles = [socket, network], args = [remote_address = address(remote_address)], delegate = |state: &mut T| HostTcpSocket::start_connect(&mut state.sockets(), socket, network, remote_address))
    }

    fn finish_connect(
        &mut self,
        socket: Resource<TcpSocket>,
    ) -> Result<(Resource<InputStream>, Resource<OutputStream>), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.finish-connect", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::finish_connect(&mut state.sockets(), socket), produced = |value: &(Resource<InputStream>, Resource<OutputStream>)| vec![value.0.rep(), value.1.rep()])
    }

    async fn start_listen(&mut self, socket: Resource<TcpSocket>) -> Result<(), SocketError> {
        gate!(socket_state_async self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.start-listen", handles = [socket], args = (), delegate = async |state: &mut T| HostTcpSocket::start_listen(&mut state.sockets(), socket).await)
    }

    fn finish_listen(&mut self, socket: Resource<TcpSocket>) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.finish-listen", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::finish_listen(&mut state.sockets(), socket))
    }

    fn accept(
        &mut self,
        socket: Resource<TcpSocket>,
    ) -> Result<
        (
            Resource<TcpSocket>,
            Resource<InputStream>,
            Resource<OutputStream>,
        ),
        SocketError,
    > {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.accept", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::accept(&mut state.sockets(), socket), produced = |value: &(Resource<TcpSocket>, Resource<InputStream>, Resource<OutputStream>)| vec![value.0.rep(), value.1.rep(), value.2.rep()])
    }

    fn local_address(
        &mut self,
        socket: Resource<TcpSocket>,
    ) -> Result<IpSocketAddress, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.local-address", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::local_address(&mut state.sockets(), socket))
    }

    fn remote_address(
        &mut self,
        socket: Resource<TcpSocket>,
    ) -> Result<IpSocketAddress, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.remote-address", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::remote_address(&mut state.sockets(), socket))
    }

    fn is_listening(&mut self, socket: Resource<TcpSocket>) -> wasmtime::Result<bool> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.is-listening", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::is_listening(&mut state.sockets(), socket))
    }

    fn address_family(&mut self, socket: Resource<TcpSocket>) -> wasmtime::Result<IpAddressFamily> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.address-family", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::address_family(&mut state.sockets(), socket))
    }

    fn set_listen_backlog_size(
        &mut self,
        socket: Resource<TcpSocket>,
        value: u64,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.set-listen-backlog-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_listen_backlog_size(&mut state.sockets(), socket, value))
    }

    fn keep_alive_enabled(&mut self, socket: Resource<TcpSocket>) -> Result<bool, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.keep-alive-enabled", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::keep_alive_enabled(&mut state.sockets(), socket))
    }

    fn set_keep_alive_enabled(
        &mut self,
        socket: Resource<TcpSocket>,
        value: bool,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.set-keep-alive-enabled", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_keep_alive_enabled(&mut state.sockets(), socket, value))
    }

    fn keep_alive_idle_time(
        &mut self,
        socket: Resource<TcpSocket>,
    ) -> Result<Duration, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.keep-alive-idle-time", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::keep_alive_idle_time(&mut state.sockets(), socket))
    }

    fn set_keep_alive_idle_time(
        &mut self,
        socket: Resource<TcpSocket>,
        value: Duration,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.set-keep-alive-idle-time", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_keep_alive_idle_time(&mut state.sockets(), socket, value))
    }

    fn keep_alive_interval(
        &mut self,
        socket: Resource<TcpSocket>,
    ) -> Result<Duration, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.keep-alive-interval", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::keep_alive_interval(&mut state.sockets(), socket))
    }

    fn set_keep_alive_interval(
        &mut self,
        socket: Resource<TcpSocket>,
        value: Duration,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.set-keep-alive-interval", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_keep_alive_interval(&mut state.sockets(), socket, value))
    }

    fn keep_alive_count(&mut self, socket: Resource<TcpSocket>) -> Result<u32, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.keep-alive-count", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::keep_alive_count(&mut state.sockets(), socket))
    }

    fn set_keep_alive_count(
        &mut self,
        socket: Resource<TcpSocket>,
        value: u32,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.set-keep-alive-count", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_keep_alive_count(&mut state.sockets(), socket, value))
    }

    fn hop_limit(&mut self, socket: Resource<TcpSocket>) -> Result<u8, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.hop-limit", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::hop_limit(&mut state.sockets(), socket))
    }

    fn set_hop_limit(&mut self, socket: Resource<TcpSocket>, value: u8) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.set-hop-limit", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_hop_limit(&mut state.sockets(), socket, value))
    }

    fn receive_buffer_size(&mut self, socket: Resource<TcpSocket>) -> Result<u64, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.receive-buffer-size", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::receive_buffer_size(&mut state.sockets(), socket))
    }

    fn set_receive_buffer_size(
        &mut self,
        socket: Resource<TcpSocket>,
        value: u64,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.set-receive-buffer-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_receive_buffer_size(&mut state.sockets(), socket, value))
    }

    fn send_buffer_size(&mut self, socket: Resource<TcpSocket>) -> Result<u64, SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.send-buffer-size", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::send_buffer_size(&mut state.sockets(), socket))
    }

    fn set_send_buffer_size(
        &mut self,
        socket: Resource<TcpSocket>,
        value: u64,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.set-send-buffer-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_send_buffer_size(&mut state.sockets(), socket, value))
    }

    fn subscribe(&mut self, socket: Resource<TcpSocket>) -> wasmtime::Result<Resource<Pollable>> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.subscribe", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::subscribe(&mut state.sockets(), socket), produced = produced_resource)
    }

    fn shutdown(
        &mut self,
        socket: Resource<TcpSocket>,
        shutdown_type: ShutdownType,
    ) -> Result<(), SocketError> {
        gate!(socket self, WASI_VERSION, "wasi:sockets/tcp", "[method]tcp-socket.shutdown", handles = [socket], args = [shutdown_type = ArgumentValue::Debug(format!("{shutdown_type:?}"))], delegate = |state: &mut T| HostTcpSocket::shutdown(&mut state.sockets(), socket, shutdown_type))
    }

    fn drop(&mut self, socket: Resource<TcpSocket>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, "wasi:sockets/tcp", "[resource-drop]tcp-socket", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::drop(&mut state.sockets(), socket))
    }
}

pub(super) fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    tcp::add_to_linker::<T, GateData<T>>(linker, project::<T>)
}
