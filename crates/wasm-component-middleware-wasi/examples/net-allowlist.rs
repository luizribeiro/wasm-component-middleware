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
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../../support/fixtures/wasi-p2/wit",
    world: "workload",
});

mod p3 {
    wasmtime::component::bindgen!({
        path: "../../support/fixtures/wasi-p3/wit",
        world: "workload",
        imports: { default: async | store },
        exports: { default: async | store },
        with: { "wasi": wasmtime_wasi::p3::bindings },
        require_store_data_send: true,
    });
}

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
            "[method]tcp-socket.connect" => call
                .args
                .get("remote_address")
                .is_some_and(|address| self.allows_address(address)),
            "[method]udp-socket.connect" => call
                .args
                .get("remote_address")
                .is_some_and(|address| self.allows_address(address)),
            "[method]udp-socket.send" => call
                .args
                .get("remote_address")
                .is_some_and(|address| self.allows_optional_address(address)),
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

struct Servers {
    allowed_address: SocketAddr,
    denied_address: SocketAddr,
    allowed: TcpListener,
    denied: TcpListener,
    allowed_udp: UdpSocket,
    denied_udp: UdpSocket,
}

struct RunningServers {
    allowed_address: SocketAddr,
    denied_address: SocketAddr,
    echo: thread::JoinHandle<std::io::Result<()>>,
    receive_datagram: thread::JoinHandle<std::io::Result<()>>,
    _denied: TcpListener,
    _denied_udp: UdpSocket,
}

impl Servers {
    fn start() -> std::io::Result<Self> {
        let allowed = listener()?;
        let denied = listener()?;
        let allowed_address = allowed.local_addr()?;
        let denied_address = denied.local_addr()?;
        let allowed_udp = UdpSocket::bind(allowed_address)?;
        let denied_udp = UdpSocket::bind(denied_address)?;
        Ok(Self {
            allowed_address,
            denied_address,
            allowed,
            denied,
            allowed_udp,
            denied_udp,
        })
    }

    fn run(self) -> std::io::Result<RunningServers> {
        self.allowed.set_nonblocking(true)?;
        self.allowed_udp
            .set_read_timeout(Some(Duration::from_secs(10)))?;
        let echo = thread::spawn(move || -> std::io::Result<()> {
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            let (mut stream, _) = loop {
                match self.allowed.accept() {
                    Ok(connection) => break connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::TimedOut,
                                "TCP client did not connect",
                            ));
                        }
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => return Err(error),
                }
            };
            stream.set_read_timeout(Some(Duration::from_secs(10)))?;
            stream.set_write_timeout(Some(Duration::from_secs(10)))?;
            let mut message = [0; 5];
            stream.read_exact(&mut message)?;
            stream.write_all(&message)
        });
        let receive_datagram = thread::spawn(move || -> std::io::Result<()> {
            let mut message = [0; 3];
            let received = self.allowed_udp.recv(&mut message)?;
            if received == message.len() && message == *b"udp" {
                Ok(())
            } else {
                Err(std::io::Error::other("unexpected UDP datagram"))
            }
        });
        Ok(RunningServers {
            allowed_address: self.allowed_address,
            denied_address: self.denied_address,
            echo,
            receive_datagram,
            _denied: self.denied,
            _denied_udp: self.denied_udp,
        })
    }
}

impl RunningServers {
    fn finish(self) -> wasmtime::Result<()> {
        self.echo
            .join()
            .map_err(|_| wasmtime::Error::msg("echo server panicked"))??;
        self.receive_datagram
            .join()
            .map_err(|_| wasmtime::Error::msg("UDP receiver panicked"))??;
        Ok(())
    }
}

fn wasi() -> WasiCtx {
    let mut wasi = WasiCtxBuilder::new();
    wasi.allow_tcp(true)
        .allow_udp(true)
        .allow_ip_name_lookup(true)
        .socket_addr_check(|address, _| Box::pin(async move { address.ip().is_loopback() }));
    wasi.build()
}

fn run_p2(servers: Servers) -> wasmtime::Result<String> {
    let engine = Engine::default();
    let component = Component::from_file(&engine, guest_build::wasi_p2())?;
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker)?;
    let chain = Chain::builder()
        .layer(Logger::stderr())
        .layer(AllowAddress(servers.allowed_address))
        .build();
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(
                std::sync::Arc::clone(&chain),
                InvocationContext::new("net-allowlist-p2"),
            ),
            table: ResourceTable::new(),
            wasi: wasi(),
        },
    );
    let guest = Workload::instantiate(&mut store, &component, &linker)?;
    let servers = servers.run()?;
    let output = guest.call_net_allowlist(
        &mut store,
        servers.allowed_address.port(),
        servers.denied_address.port(),
    )?;
    servers.finish()?;
    Ok(output)
}

async fn run_p3(servers: Servers) -> wasmtime::Result<String> {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, guest_build::wasi_p3())?;
    let mut linker = Linker::new(&engine);
    wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker)?;
    wasm_component_middleware_wasi::p3::add_to_linker(&mut linker)?;
    let chain = Chain::builder()
        .layer(Logger::stderr())
        .layer(AllowAddress(servers.allowed_address))
        .build();
    let mut store = Store::new(
        &engine,
        State {
            middleware: MiddlewareCtx::new(
                std::sync::Arc::clone(&chain),
                InvocationContext::new("net-allowlist-p3"),
            ),
            table: ResourceTable::new(),
            wasi: wasi(),
        },
    );
    let guest = p3::Workload::instantiate_async(&mut store, &component, &linker).await?;
    let servers = servers.run()?;
    let output = store
        .run_concurrent(async |accessor| {
            guest
                .call_net_allowlist(
                    accessor,
                    servers.allowed_address.port(),
                    servers.denied_address.port(),
                )
                .await
        })
        .await??;
    servers.finish()?;
    Ok(output)
}

fn main() -> wasmtime::Result<()> {
    print!("p2:\n{}", run_p2(Servers::start()?)?);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    print!("p3:\n{}", runtime.block_on(run_p3(Servers::start()?))?);
    Ok(())
}
