#![allow(missing_docs)]

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, UdpSocket};
use std::thread;
use std::time::Duration;

use wasm_component_middleware::{
    ArgumentValue, Call, Chain, Denied, InvocationContext, Layer, Logger, MiddlewareCtx,
    MiddlewareView, Outcome,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../../guests/wasi-p2/wit",
    world: "workload",
});

struct State {
    middleware: MiddlewareCtx<Self>,
    table: ResourceTable,
    wasi: WasiCtx,
}

impl MiddlewareView for State {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        &mut self.middleware
    }
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.table,
        }
    }
}

struct AllowAddress(SocketAddr);

impl AllowAddress {
    fn allows_address(&self, value: &ArgumentValue) -> bool {
        value
            .as_str()
            .and_then(|address| address.parse::<SocketAddr>().ok())
            .is_some_and(|address| address == self.0)
    }

    fn allows_optional_address(&self, value: &ArgumentValue) -> bool {
        match value {
            ArgumentValue::Variant {
                case: "none",
                value: None,
            } => true,
            ArgumentValue::Variant {
                case: "some",
                value: Some(address),
            } => self.allows_address(address),
            _ => false,
        }
    }

    fn allows_datagrams(&self, value: &ArgumentValue) -> bool {
        let ArgumentValue::List(datagrams) = value else {
            return false;
        };
        datagrams.iter().all(|datagram| {
            let ArgumentValue::List(fields) = datagram else {
                return false;
            };
            let [_, remote_address] = fields.as_slice() else {
                return false;
            };
            self.allows_optional_address(remote_address)
        })
    }
}

impl Layer<State> for AllowAddress {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        let allowed = match call.function {
            "[method]tcp-socket.start-connect" => call
                .args
                .get("remote_address")
                .is_some_and(|address| self.allows_address(address)),
            "[method]udp-socket.stream" => call
                .args
                .get("remote_address")
                .is_some_and(|address| self.allows_optional_address(address)),
            "[method]outgoing-datagram-stream.send" => call
                .args
                .get("datagrams")
                .is_some_and(|datagrams| self.allows_datagrams(datagrams)),
            _ => return Ok(()),
        };
        if allowed {
            Ok(())
        } else {
            Err(Denied::new("remote address is not allowed"))
        }
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

fn listener() -> std::io::Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
}

fn main() -> wasmtime::Result<()> {
    let allowed = listener()?;
    let denied = listener()?;
    let allowed_address = allowed.local_addr()?;
    let denied_address = denied.local_addr()?;
    let allowed_udp = UdpSocket::bind(allowed_address)?;
    let _denied_udp = UdpSocket::bind(denied_address)?;
    allowed_udp.set_read_timeout(Some(Duration::from_secs(5)))?;
    let echo = thread::spawn(move || -> std::io::Result<()> {
        let (mut stream, _) = allowed.accept()?;
        let mut message = [0; 5];
        stream.read_exact(&mut message)?;
        stream.write_all(&message)
    });
    let receive_datagram = thread::spawn(move || -> std::io::Result<()> {
        let mut message = [0; 3];
        let received = allowed_udp.recv(&mut message)?;
        if received == message.len() && message == *b"udp" {
            Ok(())
        } else {
            Err(std::io::Error::other("unexpected UDP datagram"))
        }
    });

    let engine = Engine::default();
    let component = Component::from_file(&engine, test_guests::wasi_p2())?;
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker)?;
    let chain = Chain::builder()
        .layer(Logger::stderr())
        .layer(AllowAddress(allowed_address))
        .build();
    let mut wasi = WasiCtxBuilder::new();
    wasi.allow_tcp(true)
        .allow_udp(true)
        .allow_ip_name_lookup(true)
        .socket_addr_check(|address, _| Box::pin(async move { address.ip().is_loopback() }));
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("net-allowlist")),
            table: ResourceTable::new(),
            wasi: wasi.build(),
        },
    );
    let guest = Workload::instantiate(&mut store, &component, &linker)?;
    let output =
        guest.call_net_allowlist(&mut store, allowed_address.port(), denied_address.port())?;

    echo.join()
        .map_err(|_| wasmtime::Error::msg("echo server panicked"))??;
    receive_datagram
        .join()
        .map_err(|_| wasmtime::Error::msg("UDP receiver panicked"))??;
    print!("{output}");
    Ok(())
}
