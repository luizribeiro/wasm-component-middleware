use wasm_component_middleware::MiddlewareView;
use wasmtime::AsContextMut;
use wasmtime::component::{Access, Accessor, FutureReader, Resource, StreamReader};
use wasmtime_wasi::WasiView;
use wasmtime_wasi::p3::bindings::sockets::types::{
    self, HostTcpSocket, HostTcpSocketWithStore, IpAddressFamily, IpSocketAddress, TcpSocket,
};
use wasmtime_wasi::p3::sockets::SocketResult;
use wasmtime_wasi::sockets::{WasiSockets, WasiSocketsView as _};

use crate::gate::{Gate, GateData, gate, produced_resource};

use super::super::WASI_VERSION;
use super::super::relay::{Origin, RelayMode, relay_bytes, relay_completion};
use super::address;

const INTERFACE: &str = "wasi:sockets/types";

fn delegate_access<'a, T, M>(
    store: &'a mut Access<'_, T, GateData<T, M>>,
) -> Access<'a, T, WasiSockets>
where
    T: WasiView + MiddlewareView + 'static,
    M: 'static,
{
    Access::new(store.as_context_mut(), |state: &mut T| state.sockets())
}

async fn delegate_listen<T, M>(
    store: &mut Access<'_, T, GateData<T, M>>,
    socket: Resource<TcpSocket>,
) -> wasmtime::Result<(
    StreamReader<Resource<TcpSocket>>,
    wasm_component_middleware::Completion,
)>
where
    T: WasiView + MiddlewareView + 'static,
    M: 'static,
{
    Ok((
        HostTcpSocketWithStore::listen(delegate_access(store), socket).await?,
        wasm_component_middleware::Completion::default(),
    ))
}

impl<T> types::Host for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    fn convert_error_code(
        &mut self,
        error: wasmtime_wasi::p3::sockets::SocketError,
    ) -> wasmtime::Result<types::ErrorCode> {
        types::Host::convert_error_code(&mut self.state.sockets(), error)
    }
}

impl<T> HostTcpSocket for Gate<'_, T>
where
    T: WasiView + MiddlewareView + 'static,
{
    async fn bind(
        &mut self,
        socket: Resource<TcpSocket>,
        local_address: IpSocketAddress,
    ) -> SocketResult<()> {
        gate!(socket3_state_async self, WASI_VERSION, INTERFACE, "[method]tcp-socket.bind", handles = [socket], args = [local_address = address(local_address)], delegate = async |state: &mut T| {
            let mut sockets = state.sockets();
            HostTcpSocket::bind(&mut sockets, socket, local_address).await
        })
    }

    fn create(&mut self, address_family: IpAddressFamily) -> SocketResult<Resource<TcpSocket>> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[static]tcp-socket.create", handles = [], args = (address_family), delegate = |state: &mut T| HostTcpSocket::create(&mut state.sockets(), address_family), produced = produced_resource)
    }

    fn get_local_address(&mut self, socket: Resource<TcpSocket>) -> SocketResult<IpSocketAddress> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-local-address", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_local_address(&mut state.sockets(), socket))
    }

    fn get_remote_address(&mut self, socket: Resource<TcpSocket>) -> SocketResult<IpSocketAddress> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-remote-address", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_remote_address(&mut state.sockets(), socket))
    }

    fn get_is_listening(&mut self, socket: Resource<TcpSocket>) -> wasmtime::Result<bool> {
        gate!(trap self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-is-listening", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_is_listening(&mut state.sockets(), socket))
    }

    fn get_address_family(
        &mut self,
        socket: Resource<TcpSocket>,
    ) -> wasmtime::Result<IpAddressFamily> {
        gate!(trap self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-address-family", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_address_family(&mut state.sockets(), socket))
    }

    fn set_listen_backlog_size(
        &mut self,
        socket: Resource<TcpSocket>,
        value: u64,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.set-listen-backlog-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_listen_backlog_size(&mut state.sockets(), socket, value))
    }

    fn get_keep_alive_enabled(&mut self, socket: Resource<TcpSocket>) -> SocketResult<bool> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-keep-alive-enabled", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_keep_alive_enabled(&mut state.sockets(), socket))
    }

    fn set_keep_alive_enabled(
        &mut self,
        socket: Resource<TcpSocket>,
        value: bool,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.set-keep-alive-enabled", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_keep_alive_enabled(&mut state.sockets(), socket, value))
    }

    fn get_keep_alive_idle_time(
        &mut self,
        socket: Resource<TcpSocket>,
    ) -> SocketResult<types::Duration> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-keep-alive-idle-time", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_keep_alive_idle_time(&mut state.sockets(), socket))
    }

    fn set_keep_alive_idle_time(
        &mut self,
        socket: Resource<TcpSocket>,
        value: types::Duration,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.set-keep-alive-idle-time", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_keep_alive_idle_time(&mut state.sockets(), socket, value))
    }

    fn get_keep_alive_interval(
        &mut self,
        socket: Resource<TcpSocket>,
    ) -> SocketResult<types::Duration> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-keep-alive-interval", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_keep_alive_interval(&mut state.sockets(), socket))
    }

    fn set_keep_alive_interval(
        &mut self,
        socket: Resource<TcpSocket>,
        value: types::Duration,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.set-keep-alive-interval", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_keep_alive_interval(&mut state.sockets(), socket, value))
    }

    fn get_keep_alive_count(&mut self, socket: Resource<TcpSocket>) -> SocketResult<u32> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-keep-alive-count", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_keep_alive_count(&mut state.sockets(), socket))
    }

    fn set_keep_alive_count(
        &mut self,
        socket: Resource<TcpSocket>,
        value: u32,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.set-keep-alive-count", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_keep_alive_count(&mut state.sockets(), socket, value))
    }

    fn get_hop_limit(&mut self, socket: Resource<TcpSocket>) -> SocketResult<u8> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-hop-limit", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_hop_limit(&mut state.sockets(), socket))
    }

    fn set_hop_limit(&mut self, socket: Resource<TcpSocket>, value: u8) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.set-hop-limit", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_hop_limit(&mut state.sockets(), socket, value))
    }

    fn get_receive_buffer_size(&mut self, socket: Resource<TcpSocket>) -> SocketResult<u64> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-receive-buffer-size", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_receive_buffer_size(&mut state.sockets(), socket))
    }

    fn set_receive_buffer_size(
        &mut self,
        socket: Resource<TcpSocket>,
        value: u64,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.set-receive-buffer-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_receive_buffer_size(&mut state.sockets(), socket, value))
    }

    fn get_send_buffer_size(&mut self, socket: Resource<TcpSocket>) -> SocketResult<u64> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.get-send-buffer-size", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::get_send_buffer_size(&mut state.sockets(), socket))
    }

    fn set_send_buffer_size(
        &mut self,
        socket: Resource<TcpSocket>,
        value: u64,
    ) -> SocketResult<()> {
        gate!(socket3 self, WASI_VERSION, INTERFACE, "[method]tcp-socket.set-send-buffer-size", handles = [socket], args = [value = value], delegate = |state: &mut T| HostTcpSocket::set_send_buffer_size(&mut state.sockets(), socket, value))
    }

    fn drop(&mut self, socket: Resource<TcpSocket>) -> wasmtime::Result<()> {
        gate!(trap self, WASI_VERSION, INTERFACE, "[resource-drop]tcp-socket", handles = [socket], args = (), delegate = |state: &mut T| HostTcpSocket::drop(&mut state.sockets(), socket))
    }
}

impl<T, M> HostTcpSocketWithStore<T> for GateData<T, M>
where
    T: WasiView + MiddlewareView + 'static,
    M: RelayMode,
{
    async fn connect(
        store: &Accessor<T, Self>,
        socket: Resource<TcpSocket>,
        remote_address: IpSocketAddress,
    ) -> SocketResult<()> {
        let delegate = store.with_getter::<WasiSockets>(|state: &mut T| state.sockets());
        gate!(socket3_async store, INTERFACE, "[method]tcp-socket.connect", handles = [socket.rep()], args = [remote_address = address(remote_address)], delegate = HostTcpSocketWithStore::connect(&delegate, socket, remote_address))
    }

    async fn listen(
        mut store: Access<'_, T, Self>,
        socket: Resource<TcpSocket>,
    ) -> SocketResult<StreamReader<Resource<TcpSocket>>> {
        gate!(socket3_access_async store, INTERFACE, "[method]tcp-socket.listen", handles = [socket.rep()], args = (), parameters = socket, delegate = delegate_listen::<T, M>)
    }

    fn send(
        mut store: Access<'_, T, Self>,
        socket: Resource<TcpSocket>,
        data: StreamReader<u8>,
    ) -> wasmtime::Result<FutureReader<Result<(), types::ErrorCode>>> {
        let Some(capacity) = M::CAPACITY else {
            return gate!(access store, INTERFACE, "[method]tcp-socket.send", handles = [socket.rep()], args = (), delegate = |store| HostTcpSocketWithStore::send(delegate_access(store), socket, data));
        };
        let handle = socket.rep();
        let chain = std::sync::Arc::clone(store.data_mut().middleware().chain());
        let handles = [handle];
        let call = wasm_component_middleware::Call::new(
            chain.next_id(),
            wasm_component_middleware::Direction::Import,
            "[method]tcp-socket.send",
        )
        .in_interface(INTERFACE, Some(WASI_VERSION))
        .with_handles(&handles);
        chain.dispatch_access(store, &call, |store| {
            let origin = Origin {
                call_id: call.id,
                interface: INTERFACE,
                version: WASI_VERSION,
                function: "[stream-write]tcp-socket.send",
                handles: std::sync::Arc::from([handle]),
            };
            let (data, shared) = relay_bytes(
                store,
                data,
                origin,
                capacity,
                Err(types::ErrorCode::AccessDenied),
            )?;
            let completion = HostTcpSocketWithStore::send(delegate_access(store), socket, data)?;
            let completion = relay_completion(store, completion, shared)?;
            Ok((completion, wasm_component_middleware::Completion::default()))
        })
    }

    fn receive(
        mut store: Access<'_, T, Self>,
        socket: Resource<TcpSocket>,
    ) -> wasmtime::Result<(StreamReader<u8>, FutureReader<Result<(), types::ErrorCode>>)> {
        let Some(capacity) = M::CAPACITY else {
            return gate!(access store, INTERFACE, "[method]tcp-socket.receive", handles = [socket.rep()], args = (), delegate = |store| HostTcpSocketWithStore::receive(delegate_access(store), socket));
        };
        let handle = socket.rep();
        let chain = std::sync::Arc::clone(store.data_mut().middleware().chain());
        let handles = [handle];
        let call = wasm_component_middleware::Call::new(
            chain.next_id(),
            wasm_component_middleware::Direction::Import,
            "[method]tcp-socket.receive",
        )
        .in_interface(INTERFACE, Some(WASI_VERSION))
        .with_handles(&handles);
        chain.dispatch_access(store, &call, |store| {
            let (input, completion) =
                HostTcpSocketWithStore::receive(delegate_access(store), socket)?;
            let origin = Origin {
                call_id: call.id,
                interface: INTERFACE,
                version: WASI_VERSION,
                function: "[stream-read]tcp-socket.receive",
                handles: std::sync::Arc::from([handle]),
            };
            let (output, shared) = relay_bytes(
                store,
                input,
                origin,
                capacity,
                Err(types::ErrorCode::AccessDenied),
            )?;
            let completion = relay_completion(store, completion, shared)?;
            Ok((
                (output, completion),
                wasm_component_middleware::Completion::default(),
            ))
        })
    }
}
