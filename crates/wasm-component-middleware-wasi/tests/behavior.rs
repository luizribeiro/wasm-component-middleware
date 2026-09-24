#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::fs;
use std::future::poll_fn;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::{Duration, Instant};

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWrite};
use wasm_component_middleware::{
    ArgumentValue, Budget, Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx,
    MiddlewareView, Outcome,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::cli::{IsTerminal, StdinStream, StdoutStream};
use wasmtime_wasi::p2::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::p2::{InputStream, OutputStream, Pollable, StreamResult};
use wasmtime_wasi::random::Deterministic;
use wasmtime_wasi::{
    FsPerms, HostMonotonicClock, HostWallClock, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView,
};
use wit_parser::{Resolve, TypeDefKind};

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

mod p2_async {
    wasmtime::component::bindgen!({
        path: "../../support/fixtures/wasi-p2/wit",
        world: "workload",
        exports: { default: async },
        require_store_data_send: true,
    });
}

mod sandbox_guest {
    wasmtime::component::bindgen!({
        path: "../../support/fixtures/sandbox/wit",
        world: "sandbox",
    });
}

struct State {
    middleware: Option<MiddlewareCtx<Self>>,
    table: ResourceTable,
    wasi: WasiCtx,
}

impl MiddlewareView for State {
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
        self.middleware.as_mut().unwrap()
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

struct FixedWallClock;

impl HostWallClock for FixedWallClock {
    fn resolution(&self) -> Duration {
        Duration::from_nanos(1)
    }

    fn now(&self) -> Duration {
        Duration::new(1_700_000_000, 123_456_789)
    }
}

struct FixedMonotonicClock;

impl HostMonotonicClock for FixedMonotonicClock {
    fn resolution(&self) -> u64 {
        1
    }

    fn now(&self) -> u64 {
        42
    }
}

struct ProgrammableMonotonicClock(Arc<AtomicU64>);

impl HostMonotonicClock for ProgrammableMonotonicClock {
    fn resolution(&self) -> u64 {
        1
    }

    fn now(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

#[derive(Clone, Default)]
struct IoTrace(Arc<Mutex<Vec<String>>>);

impl IoTrace {
    fn record(&self, event: impl Into<String>) {
        self.0.lock().unwrap().push(event.into());
    }

    fn events(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Clone)]
struct TracedStdin {
    pipe: MemoryInputPipe,
    trace: IoTrace,
}

impl IsTerminal for TracedStdin {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdinStream for TracedStdin {
    fn async_stream(&self) -> Box<dyn AsyncRead + Send + Sync> {
        Box::new(self.pipe.clone())
    }

    fn p2_stream(&self) -> Box<dyn InputStream> {
        Box::new(TracedInput {
            pipe: self.pipe.clone(),
            trace: self.trace.clone(),
            pending_once: true,
        })
    }
}

struct TracedInput {
    pipe: MemoryInputPipe,
    trace: IoTrace,
    pending_once: bool,
}

#[wasmtime_wasi::async_trait]
impl Pollable for TracedInput {
    async fn ready(&mut self) {
        poll_fn(|waker| {
            self.trace.record("stdin:ready-poll");
            if self.pending_once {
                self.pending_once = false;
                waker.waker().wake_by_ref();
                Poll::Pending
            } else {
                self.pending_once = true;
                Poll::Ready(())
            }
        })
        .await;
    }
}

#[wasmtime_wasi::async_trait]
impl InputStream for TracedInput {
    fn read(&mut self, size: usize) -> StreamResult<Bytes> {
        self.trace.record(format!("stdin:read:{size}"));
        InputStream::read(&mut self.pipe, size)
    }

    async fn blocking_read(&mut self, size: usize) -> StreamResult<Bytes> {
        self.trace.record(format!("stdin:blocking-read:{size}"));
        InputStream::read(&mut self.pipe, size)
    }

    fn skip(&mut self, size: usize) -> StreamResult<usize> {
        self.trace.record(format!("stdin:skip:{size}"));
        InputStream::read(&mut self.pipe, size).map(|bytes| bytes.len())
    }

    async fn blocking_skip(&mut self, size: usize) -> StreamResult<usize> {
        self.trace.record(format!("stdin:blocking-skip:{size}"));
        InputStream::read(&mut self.pipe, size).map(|bytes| bytes.len())
    }
}

#[derive(Clone)]
struct TracedStdout {
    label: &'static str,
    pipe: MemoryOutputPipe,
    trace: IoTrace,
}

impl IsTerminal for TracedStdout {
    fn is_terminal(&self) -> bool {
        false
    }
}

impl StdoutStream for TracedStdout {
    fn async_stream(&self) -> Box<dyn AsyncWrite + Send + Sync> {
        Box::new(self.pipe.clone())
    }

    fn p2_stream(&self) -> Box<dyn OutputStream> {
        Box::new(TracedOutput {
            label: self.label,
            pipe: self.pipe.clone(),
            trace: self.trace.clone(),
            pending_once: true,
        })
    }
}

struct TracedOutput {
    label: &'static str,
    pipe: MemoryOutputPipe,
    trace: IoTrace,
    pending_once: bool,
}

impl TracedOutput {
    fn record(&self, operation: &str) {
        self.trace.record(format!("{}:{operation}", self.label));
    }
}

#[wasmtime_wasi::async_trait]
impl Pollable for TracedOutput {
    async fn ready(&mut self) {
        poll_fn(|waker| {
            self.trace.record(format!("{}:ready-poll", self.label));
            if self.pending_once {
                self.pending_once = false;
                waker.waker().wake_by_ref();
                Poll::Pending
            } else {
                self.pending_once = true;
                Poll::Ready(())
            }
        })
        .await;
    }
}

#[wasmtime_wasi::async_trait]
impl OutputStream for TracedOutput {
    fn write(&mut self, bytes: Bytes) -> StreamResult<()> {
        self.record(&format!("write:{}", bytes.len()));
        OutputStream::write(&mut self.pipe, bytes)
    }

    fn flush(&mut self) -> StreamResult<()> {
        self.record("flush");
        OutputStream::flush(&mut self.pipe)
    }

    fn check_write(&mut self) -> StreamResult<usize> {
        self.record("check-write");
        OutputStream::check_write(&mut self.pipe)
    }

    async fn blocking_write_and_flush(&mut self, bytes: Bytes) -> StreamResult<()> {
        self.record(&format!("blocking-write-and-flush:{}", bytes.len()));
        OutputStream::write(&mut self.pipe, bytes)?;
        OutputStream::flush(&mut self.pipe)
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Observation {
    returned: String,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    io: Vec<String>,
}

struct Harness {
    store: Store<State>,
    guest: Workload,
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
    io: IoTrace,
    _directory: tempfile::TempDir,
}

struct P3Harness {
    store: Store<State>,
    guest: p3::Workload,
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
    monotonic_now: Arc<AtomicU64>,
    directory: tempfile::TempDir,
}

struct AsyncP2Harness {
    store: Store<State>,
    guest: p2_async::Workload,
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
    io: IoTrace,
    _directory: tempfile::TempDir,
}

struct P2Fixture {
    state: State,
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
    io: IoTrace,
    directory: tempfile::TempDir,
}

fn preopen_fixture(builder: &mut WasiCtxBuilder) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.txt"), b"alpha-file").unwrap();
    fs::write(directory.path().join("other.txt"), b"other-file").unwrap();
    fs::create_dir(directory.path().join("empty-dir")).unwrap();
    builder
        .preopened_dir(directory.path(), ".", FsPerms::ReadWrite)
        .unwrap();
    directory
}

impl P3Harness {
    async fn new(gated: bool, chain: Arc<Chain<State>>) -> wasmtime::Result<Self> {
        Self::new_with_linking(gated, false, false, chain).await
    }

    async fn new_relayed(chain: Arc<Chain<State>>) -> wasmtime::Result<Self> {
        Self::new_with_linking(true, true, false, chain).await
    }

    async fn new_for_benchmark(relayed: bool, blocking: bool) -> wasmtime::Result<Self> {
        Self::new_with_linking(true, relayed, blocking, Chain::builder().build()).await
    }

    async fn new_with_linking(
        gated: bool,
        relayed: bool,
        blocking: bool,
        chain: Arc<Chain<State>>,
    ) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model_async(true);
        config.concurrency_support(true);
        let engine = Engine::new(&config)?;
        let component = Component::from_file(&engine, guest_build::wasi_p3())?;
        let mut linker = Linker::new(&engine);
        wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker)?;
        if relayed {
            wasm_component_middleware_wasi::p3::add_to_linker_with_stream_relay(
                &mut linker,
                wasm_component_middleware_wasi::p3::StreamRelay::default(),
            )?;
        } else if gated {
            wasm_component_middleware_wasi::p3::add_to_linker(&mut linker)?;
        } else {
            wasmtime_wasi::p3::add_to_linker(&mut linker)?;
        }

        let stdout = MemoryOutputPipe::new(4096);
        let stderr = MemoryOutputPipe::new(4096);
        let monotonic_now = Arc::new(AtomicU64::new(10_000_000_000));
        let mut builder = WasiCtxBuilder::new();
        builder
            .allow_blocking_current_thread(blocking)
            .env("GREETING", "Hello")
            .args(&["program", "one"])
            .stdin(MemoryInputPipe::new("p3 input"))
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .secure_random(Deterministic::new(vec![1, 2, 3, 4]))
            .insecure_random(Deterministic::new(vec![9, 10, 11, 12]))
            .insecure_random_seed(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff)
            .wall_clock(FixedWallClock)
            .monotonic_clock(ProgrammableMonotonicClock(Arc::clone(&monotonic_now)))
            .allow_tcp(true)
            .allow_udp(true)
            .allow_ip_name_lookup(true)
            .socket_addr_check(|address, _| Box::pin(async move { address.ip().is_loopback() }));
        let directory = preopen_fixture(&mut builder);
        let mut store = Store::new(
            &engine,
            State {
                middleware: Some(MiddlewareCtx::new(
                    chain,
                    InvocationContext::new("workload"),
                )),
                table: ResourceTable::new(),
                wasi: builder.build(),
            },
        );
        let guest = p3::Workload::instantiate_async(&mut store, &component, &linker).await?;
        Ok(Self {
            store,
            guest,
            stdout,
            stderr,
            monotonic_now,
            directory,
        })
    }

    fn set_monotonic_now(&self, now: u64) {
        self.monotonic_now.store(now, Ordering::Relaxed);
    }

    async fn basic_system(&mut self) -> wasmtime::Result<()> {
        self.store
            .run_concurrent(async |accessor| self.guest.call_basic_system(accessor).await)
            .await??;
        Ok(())
    }

    async fn common_calls(&mut self) -> wasmtime::Result<()> {
        self.store
            .run_concurrent(async |accessor| self.guest.call_common_calls(accessor).await)
            .await??;
        Ok(())
    }

    async fn cancel_wait(&mut self) -> wasmtime::Result<()> {
        self.store
            .run_concurrent(async |accessor| self.guest.call_cancel_wait(accessor).await)
            .await??;
        Ok(())
    }

    async fn abandon_wait(&mut self) -> wasmtime::Result<()> {
        self.store
            .run_concurrent(async |accessor| self.guest.call_abandon_wait(accessor).await)
            .await??;
        Ok(())
    }

    async fn exercise(&mut self) -> wasmtime::Result<String> {
        let stdin = self
            .store
            .run_concurrent(async |accessor| self.guest.call_exercise(accessor).await)
            .await??;
        Ok(stdin)
    }

    async fn exit(&mut self, with_code: bool) -> wasmtime::Result<()> {
        self.store
            .run_concurrent(async |accessor| {
                if with_code {
                    self.guest.call_exit_code(accessor).await
                } else {
                    self.guest.call_exit_success(accessor).await
                }
            })
            .await??;
        Ok(())
    }

    async fn refused_open(&mut self) -> wasmtime::Result<bool> {
        let refused = self
            .store
            .run_concurrent(async |accessor| self.guest.call_refused_open(accessor).await)
            .await??;
        Ok(refused)
    }

    async fn socket_denial(&mut self, port: u16) -> wasmtime::Result<bool> {
        let refused = self
            .store
            .run_concurrent(async |accessor| self.guest.call_socket_denial(accessor, port).await)
            .await??;
        Ok(refused)
    }

    async fn tcp_bind_denial(&mut self) -> wasmtime::Result<bool> {
        let refused = self
            .store
            .run_concurrent(async |accessor| self.guest.call_tcp_bind_denial(accessor).await)
            .await??;
        Ok(refused)
    }

    async fn udp_connect_denial(&mut self, port: u16) -> wasmtime::Result<bool> {
        let refused = self
            .store
            .run_concurrent(async |accessor| {
                self.guest.call_udp_connect_denial(accessor, port).await
            })
            .await??;
        Ok(refused)
    }

    async fn tcp_send_denial(&mut self, port: u16, size: u64) -> wasmtime::Result<(u64, bool)> {
        let result = self
            .store
            .run_concurrent(async |accessor| {
                self.guest.call_tcp_send_denial(accessor, port, size).await
            })
            .await??;
        Ok(result)
    }

    async fn stream_read(&mut self, path: &str, slow: bool) -> wasmtime::Result<String> {
        let result = self
            .store
            .run_concurrent(async |accessor| {
                self.guest
                    .call_stream_read(accessor, path.to_owned(), slow)
                    .await
            })
            .await??;
        Ok(result)
    }

    async fn stream_write(&mut self, path: &str, size: u64) -> wasmtime::Result<String> {
        let result = self
            .store
            .run_concurrent(async |accessor| {
                self.guest
                    .call_stream_write(accessor, path.to_owned(), size)
                    .await
            })
            .await??;
        Ok(result)
    }

    async fn stream_write_tolerant(&mut self, path: &str, size: u64) -> wasmtime::Result<String> {
        let result = self
            .store
            .run_concurrent(async |accessor| {
                self.guest
                    .call_stream_write_tolerant(accessor, path.to_owned(), size)
                    .await
            })
            .await??;
        Ok(result)
    }

    async fn cancel_stream_read(&mut self, path: &str) -> wasmtime::Result<()> {
        self.store
            .run_concurrent(async |accessor| {
                self.guest
                    .call_cancel_stream_read(accessor, path.to_owned())
                    .await
            })
            .await??;
        Ok(())
    }

    async fn cancel_stream_write(&mut self, path: &str) -> wasmtime::Result<()> {
        self.store
            .run_concurrent(async |accessor| {
                self.guest
                    .call_cancel_stream_write(accessor, path.to_owned())
                    .await
            })
            .await??;
        Ok(())
    }
}

fn p2_fixture(chain: Arc<Chain<State>>) -> P2Fixture {
    let stdout = MemoryOutputPipe::new(4096);
    let stderr = MemoryOutputPipe::new(4096);
    let io = IoTrace::default();
    let mut builder = WasiCtxBuilder::new();
    builder
        .env("GREETING", "Hello")
        .args(&["program", "one"])
        .stdin(TracedStdin {
            pipe: MemoryInputPipe::new("guest input"),
            trace: io.clone(),
        })
        .stdout(TracedStdout {
            label: "stdout",
            pipe: stdout.clone(),
            trace: io.clone(),
        })
        .stderr(TracedStdout {
            label: "stderr",
            pipe: stderr.clone(),
            trace: io.clone(),
        })
        .secure_random(Deterministic::new(vec![1, 2, 3, 4]))
        .insecure_random(Deterministic::new(vec![9, 10, 11, 12]))
        .insecure_random_seed(0x0011_2233_4455_6677_8899_aabb_ccdd_eeff)
        .wall_clock(FixedWallClock)
        .monotonic_clock(FixedMonotonicClock)
        .allow_tcp(true)
        .allow_udp(true)
        .allow_ip_name_lookup(true)
        .socket_addr_check(|address, _| Box::pin(async move { address.ip().is_loopback() }));
    let directory = preopen_fixture(&mut builder);
    P2Fixture {
        state: State {
            middleware: Some(MiddlewareCtx::new(
                chain,
                InvocationContext::new("workload"),
            )),
            table: ResourceTable::new(),
            wasi: builder.build(),
        },
        stdout,
        stderr,
        io,
        directory,
    }
}

impl Harness {
    fn new(gated: bool, chain: Arc<Chain<State>>) -> wasmtime::Result<Self> {
        let engine = Engine::default();
        let component = Component::from_file(&engine, guest_build::wasi_p2())?;
        let mut linker = Linker::new(&engine);
        if gated {
            wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker)?;
        } else {
            wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        }
        let fixture = p2_fixture(chain);
        let mut store = Store::new(&engine, fixture.state);
        let guest = Workload::instantiate(&mut store, &component, &linker)?;
        Ok(Self {
            store,
            guest,
            stdout: fixture.stdout,
            stderr: fixture.stderr,
            io: fixture.io,
            _directory: fixture.directory,
        })
    }

    fn churn_files(&mut self) -> wasmtime::Result<()> {
        self.guest.call_churn_files(&mut self.store)
    }

    fn rust_std_paths(&mut self) -> wasmtime::Result<u32> {
        self.guest.call_rust_std_paths(&mut self.store)
    }

    fn quota_files(&mut self) -> wasmtime::Result<String> {
        self.guest.call_quota_files(&mut self.store)
    }
}

impl AsyncP2Harness {
    async fn new(gated: bool, chain: Arc<Chain<State>>) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model_async(true);
        let engine = Engine::new(&config)?;
        let component = Component::from_file(&engine, guest_build::wasi_p2())?;
        let mut linker = Linker::new(&engine);
        if gated {
            wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker)?;
        } else {
            wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        }
        let fixture = p2_fixture(chain);
        let mut store = Store::new(&engine, fixture.state);
        let guest = p2_async::Workload::instantiate_async(&mut store, &component, &linker).await?;
        Ok(Self {
            store,
            guest,
            stdout: fixture.stdout,
            stderr: fixture.stderr,
            io: fixture.io,
            _directory: fixture.directory,
        })
    }

    async fn exercise(&mut self) -> wasmtime::Result<Observation> {
        let returned = self.guest.call_exercise(&mut self.store).await?;
        Ok(Observation {
            returned,
            stdout: self.stdout.contents().to_vec(),
            stderr: self.stderr.contents().to_vec(),
            io: self.io.events(),
        })
    }
}

fn run(gated: bool) -> wasmtime::Result<Observation> {
    let mut harness = Harness::new(gated, Chain::builder().build())?;
    let returned = harness.guest.call_observe(&mut harness.store)?;

    Ok(Observation {
        returned,
        stdout: harness.stdout.contents().to_vec(),
        stderr: harness.stderr.contents().to_vec(),
        io: harness.io.events(),
    })
}

fn run_exercise(gated: bool) -> wasmtime::Result<Observation> {
    let mut harness = Harness::new(gated, Chain::builder().build())?;
    let returned = harness.guest.call_exercise(&mut harness.store)?;

    Ok(Observation {
        returned,
        stdout: harness.stdout.contents().to_vec(),
        stderr: harness.stderr.contents().to_vec(),
        io: harness.io.events(),
    })
}

struct Refuse(&'static str);

impl Layer<State> for Refuse {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        if call.function == self.0 {
            Err(Denied::new(format!("{} is disabled", call.function)))
        } else {
            Ok(())
        }
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

#[derive(Clone)]
struct CaptureAddress(Arc<Mutex<Option<String>>>);

impl Layer<State> for CaptureAddress {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        if matches!(
            call.function,
            "[method]tcp-socket.start-connect" | "[method]tcp-socket.connect"
        ) {
            *self.0.lock().unwrap() = call
                .args
                .get("remote_address")
                .and_then(ArgumentValue::as_str)
                .map(str::to_owned);
        }
        Ok(())
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

#[derive(Clone)]
struct Record(Arc<Mutex<BTreeSet<(String, String)>>>);

impl Layer<State> for Record {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        if let Some(interface) = call.interface {
            self.0
                .lock()
                .unwrap()
                .insert((interface.to_owned(), call.function.to_owned()));
        }
        Ok(())
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

fn expected_calls() -> BTreeSet<(String, String)> {
    let routed = wasm_component_middleware_wasi::p2::ROUTED_INTERFACES
        .iter()
        .map(|interface| interface.name())
        .collect::<BTreeSet<_>>();
    let mut resolve = Resolve::default();
    let wit = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wit");
    resolve.push_dir(wit).unwrap();
    let mut calls = BTreeSet::new();

    for (_, package) in &resolve.packages {
        for (interface_name, interface_id) in &package.interfaces {
            let interface_name = format!(
                "{}:{}/{interface_name}",
                package.name.namespace, package.name.name
            );
            if !routed.contains(interface_name.as_str()) {
                continue;
            }
            let interface = &resolve.interfaces[*interface_id];
            calls.extend(
                interface
                    .functions
                    .values()
                    .map(|function| (interface_name.clone(), function.name.clone())),
            );
            calls.extend(
                interface
                    .types
                    .iter()
                    .filter(|(_, id)| matches!(resolve.types[**id].kind, TypeDefKind::Resource))
                    .map(|(name, _)| (interface_name.clone(), format!("[resource-drop]{name}"))),
            );
        }
    }
    calls
}

fn expected_p3_calls() -> BTreeSet<(String, String)> {
    let routed = wasm_component_middleware_wasi::p3::ROUTED_INTERFACES
        .iter()
        .map(|interface| interface.name())
        .collect::<BTreeSet<_>>();
    let mut resolve = Resolve::default();
    let wit = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wit-p3");
    resolve.push_dir(wit).unwrap();
    let mut calls = BTreeSet::new();

    for (_, package) in &resolve.packages {
        for (interface_name, interface_id) in &package.interfaces {
            let interface_name = format!(
                "{}:{}/{interface_name}",
                package.name.namespace, package.name.name
            );
            if !routed.contains(interface_name.as_str()) {
                continue;
            }
            let interface = &resolve.interfaces[*interface_id];
            calls.extend(
                interface
                    .functions
                    .values()
                    .map(|function| (interface_name.clone(), function.name.clone())),
            );
            calls.extend(
                interface
                    .types
                    .iter()
                    .filter(|(_, id)| matches!(resolve.types[**id].kind, TypeDefKind::Resource))
                    .map(|(name, _)| (interface_name.clone(), format!("[resource-drop]{name}"))),
            );
        }
    }
    calls
}

#[test]
fn every_callable_function_dispatches() {
    const IMPOSSIBLE: &[(&str, &str, &str)] = &[
        (
            "wasi:cli/terminal-input",
            "[resource-drop]terminal-input",
            "captured stdin is not a terminal",
        ),
        (
            "wasi:cli/terminal-output",
            "[resource-drop]terminal-output",
            "captured stdout and stderr are not terminals",
        ),
    ];

    let calls = Arc::new(Mutex::new(BTreeSet::new()));
    let recorder = Record(Arc::clone(&calls));
    let mut exercise =
        Harness::new(true, Chain::builder().layer(recorder.clone()).build()).unwrap();
    exercise.guest.call_exercise(&mut exercise.store).unwrap();

    let mut denied_write = Harness::new(
        true,
        Chain::builder()
            .layer(recorder.clone())
            .layer(Refuse("[method]output-stream.blocking-write-and-flush"))
            .build(),
    )
    .unwrap();
    assert!(
        denied_write
            .guest
            .call_write_stdout(&mut denied_write.store)
            .unwrap()
    );

    for exit_with_code in [false, true] {
        let mut exiting =
            Harness::new(true, Chain::builder().layer(recorder.clone()).build()).unwrap();
        let result = if exit_with_code {
            exiting.guest.call_exit_code(&mut exiting.store)
        } else {
            exiting.guest.call_exit_success(&mut exiting.store)
        };
        assert!(result.is_err());
    }

    let mut expected = expected_calls();
    for (interface, function, _reason) in IMPOSSIBLE {
        assert!(expected.remove(&(String::from(*interface), String::from(*function))));
    }
    assert_eq!(*calls.lock().unwrap(), expected);
}

#[tokio::test]
async fn every_callable_p3_function_dispatches() {
    const IMPOSSIBLE: &[(&str, &str)] = &[
        ("wasi:cli/terminal-input", "[resource-drop]terminal-input"),
        ("wasi:cli/terminal-output", "[resource-drop]terminal-output"),
    ];

    let calls = Arc::new(Mutex::new(BTreeSet::new()));
    let recorder = Record(Arc::clone(&calls));
    let mut exercise = P3Harness::new(true, Chain::builder().layer(recorder.clone()).build())
        .await
        .unwrap();
    exercise.exercise().await.unwrap();

    for with_code in [false, true] {
        let mut exiting = P3Harness::new(true, Chain::builder().layer(recorder.clone()).build())
            .await
            .unwrap();
        assert!(exiting.exit(with_code).await.is_err());
    }

    let mut expected = expected_p3_calls();
    for (interface, function) in IMPOSSIBLE {
        assert!(expected.remove(&(String::from(*interface), String::from(*function))));
    }
    assert_eq!(*calls.lock().unwrap(), expected);
}

#[test]
fn refusing_environment_traps_the_guest() {
    let chain = Chain::builder().layer(Refuse("get-environment")).build();
    let mut harness = Harness::new(true, chain).unwrap();
    let error = harness.guest.call_observe(&mut harness.store).unwrap_err();

    assert_eq!(
        error.downcast_ref::<Denied>().unwrap().reason(),
        "get-environment is disabled"
    );
}

#[test]
fn refusing_random_traps_the_guest() {
    let chain = Chain::builder().layer(Refuse("get-random-u64")).build();
    let mut harness = Harness::new(true, chain).unwrap();
    let error = harness.guest.call_exercise(&mut harness.store).unwrap_err();

    assert_eq!(
        error.downcast_ref::<Denied>().unwrap().reason(),
        "get-random-u64 is disabled"
    );
}

#[test]
fn refusing_tcp_connect_returns_access_and_exposes_the_address() {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let address = Arc::new(Mutex::new(None));
    let chain = Chain::builder()
        .layer(CaptureAddress(Arc::clone(&address)))
        .layer(Refuse("[method]tcp-socket.start-connect"))
        .build();
    let mut harness = Harness::new(true, chain).unwrap();

    assert!(
        harness
            .guest
            .call_socket_denial(&mut harness.store, port)
            .unwrap()
    );
    assert_eq!(
        address.lock().unwrap().as_deref(),
        Some(format!("127.0.0.1:{port}").as_str())
    );
}

#[test]
fn refusing_blocking_write_is_a_guest_visible_stream_error() {
    let chain = Chain::builder()
        .layer(Refuse("[method]output-stream.blocking-write-and-flush"))
        .build();
    let mut harness = Harness::new(true, chain).unwrap();

    assert!(harness.guest.call_write_stdout(&mut harness.store).unwrap());
}

#[test]
fn refusing_resource_drop_traps_the_guest() {
    let chain = Chain::builder()
        .layer(Refuse("[resource-drop]output-stream"))
        .build();
    let mut harness = Harness::new(true, chain).unwrap();
    let error = harness
        .guest
        .call_write_stdout(&mut harness.store)
        .unwrap_err();

    assert_eq!(
        error.downcast_ref::<Denied>().unwrap().reason(),
        "[resource-drop]output-stream is disabled"
    );
}

#[tokio::test]
async fn refusing_p3_calls_traps_the_guest() {
    for (function, exercise) in [
        ("get-environment", false),
        ("write-via-stream", false),
        ("get-random-u64", true),
    ] {
        let mut harness = P3Harness::new(true, Chain::builder().layer(Refuse(function)).build())
            .await
            .unwrap();
        let error = if exercise {
            harness.exercise().await.unwrap_err()
        } else {
            harness.basic_system().await.unwrap_err()
        };

        assert_eq!(
            error.downcast_ref::<Denied>().unwrap().reason(),
            format!("{function} is disabled")
        );
    }
}

#[tokio::test]
async fn refusing_p3_filesystem_stream_calls_traps_the_guest() {
    for function in [
        "[method]descriptor.read-via-stream",
        "[method]descriptor.write-via-stream",
        "[method]descriptor.append-via-stream",
        "[method]descriptor.read-directory",
    ] {
        let mut harness = P3Harness::new(true, Chain::builder().layer(Refuse(function)).build())
            .await
            .unwrap();
        let error = harness.exercise().await.unwrap_err();

        assert_eq!(
            error.downcast_ref::<Denied>().unwrap().reason(),
            format!("{function} is disabled")
        );
    }
}

#[tokio::test]
async fn refusing_p3_open_is_a_guest_visible_access_error() {
    let mut harness = P3Harness::new(
        true,
        Chain::builder()
            .layer(Refuse("[method]descriptor.open-at"))
            .build(),
    )
    .await
    .unwrap();

    assert!(harness.refused_open().await.unwrap());
}

#[tokio::test]
async fn refusing_p3_tcp_connect_returns_access_and_exposes_the_address() {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let address = Arc::new(Mutex::new(None));
    let chain = Chain::builder()
        .layer(CaptureAddress(Arc::clone(&address)))
        .layer(Refuse("[method]tcp-socket.connect"))
        .build();
    let mut harness = P3Harness::new(true, chain).await.unwrap();

    assert!(harness.socket_denial(port).await.unwrap());
    assert_eq!(
        address.lock().unwrap().as_deref(),
        Some(format!("127.0.0.1:{port}").as_str())
    );
}

#[tokio::test]
async fn refusing_borrowed_p3_socket_calls_returns_access() {
    let mut tcp = P3Harness::new(
        true,
        Chain::builder()
            .layer(Refuse("[method]tcp-socket.bind"))
            .build(),
    )
    .await
    .unwrap();
    assert!(tcp.tcp_bind_denial().await.unwrap());

    let mut udp = P3Harness::new(
        true,
        Chain::builder()
            .layer(Refuse("[method]udp-socket.connect"))
            .build(),
    )
    .await
    .unwrap();
    assert!(udp.udp_connect_denial(7).await.unwrap());
}

#[test]
fn gated_wasi_matches_plain_wasi() {
    assert_eq!(run(false).unwrap(), run(true).unwrap());
    let plain = run_exercise(false).unwrap();
    let gated = run_exercise(true).unwrap();

    assert_eq!(plain, gated);
    assert!(
        plain
            .returned
            .contains("random=[4, 4, 4, 4]:72623859723010820"),
        "{}",
        plain.returned
    );
    assert!(
        plain
            .returned
            .contains("insecure=[12, 12, 12, 12]:651345242427624204"),
        "{}",
        plain.returned
    );
}

#[tokio::test]
async fn async_preview_2_matches_plain_and_sync_wasi() {
    let mut plain = AsyncP2Harness::new(false, Chain::builder().build())
        .await
        .unwrap();
    let plain = plain.exercise().await.unwrap();
    let mut gated = AsyncP2Harness::new(true, Chain::builder().build())
        .await
        .unwrap();
    let gated = gated.exercise().await.unwrap();

    assert_eq!(gated, plain);
    let synchronous = tokio::task::spawn_blocking(|| run_exercise(true).unwrap())
        .await
        .unwrap();
    assert_eq!(gated, synchronous);

    let calls = Arc::new(Mutex::new(BTreeSet::new()));
    let mut recorded = AsyncP2Harness::new(
        true,
        Chain::builder().layer(Record(Arc::clone(&calls))).build(),
    )
    .await
    .unwrap();
    recorded.exercise().await.unwrap();
    let calls = calls.lock().unwrap();
    for expected in [
        ("wasi:filesystem/types", "[method]descriptor.open-at"),
        ("wasi:io/streams", "[method]input-stream.blocking-read"),
        ("wasi:sockets/ip-name-lookup", "resolve-addresses"),
        ("wasi:sockets/tcp", "[method]tcp-socket.start-connect"),
        ("wasi:sockets/udp", "[method]udp-socket.stream"),
    ] {
        assert!(
            calls.contains(&(expected.0.to_owned(), expected.1.to_owned())),
            "missing {expected:?} from {calls:?}"
        );
    }
}

#[derive(Default)]
struct DescriptorLifetimes {
    live: BTreeSet<u32>,
    seen: BTreeSet<u32>,
    opened: usize,
    recycled: bool,
}

#[derive(Clone)]
struct TrackDescriptors(Arc<Mutex<DescriptorLifetimes>>);

impl Layer<State> for TrackDescriptors {
    type Frame = bool;

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<bool, Denied> {
        if call.interface == Some("wasi:filesystem/types")
            && call.function == "[resource-drop]descriptor"
        {
            let mut lifetimes = self.0.lock().unwrap();
            for handle in call.handles {
                lifetimes.live.remove(handle);
            }
        }
        Ok(call.function == "[method]descriptor.open-at")
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, opened: bool, outcome: Outcome<'_>) {
        if let (true, Outcome::Returned(completion)) = (opened, outcome) {
            let mut lifetimes = self.0.lock().unwrap();
            for handle in &completion.produced {
                assert!(lifetimes.live.insert(*handle));
                lifetimes.opened += 1;
                if !lifetimes.seen.insert(*handle) {
                    lifetimes.recycled = true;
                }
            }
        }
    }
}

#[test]
fn descriptor_state_is_evicted_before_representations_are_recycled() {
    let lifetimes = Arc::new(Mutex::new(DescriptorLifetimes::default()));
    let mut harness = Harness::new(
        true,
        Chain::builder()
            .layer(TrackDescriptors(Arc::clone(&lifetimes)))
            .build(),
    )
    .unwrap();

    harness.churn_files().unwrap();

    let lifetimes = lifetimes.lock().unwrap();
    assert_eq!(lifetimes.opened, 128);
    assert!(lifetimes.recycled);
    assert!(lifetimes.live.is_empty());
}

#[test]
fn rust_std_fetches_preopens_once_for_several_paths() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let mut harness = Harness::new(
        true,
        Chain::builder().layer(Sequence(Arc::clone(&calls))).build(),
    )
    .unwrap();

    assert_eq!(harness.rust_std_paths().unwrap(), 30);

    let preopen_calls = calls
        .lock()
        .unwrap()
        .iter()
        .filter(|(interface, function)| {
            interface == "wasi:filesystem/preopens" && function == "get-directories"
        })
        .count();
    assert_eq!(preopen_calls, 1);
}

#[test]
fn open_file_limit_reports_access_and_releases_slots() {
    let mut harness = Harness::new(
        true,
        Chain::builder()
            .layer(wasm_component_middleware_wasi::OpenFiles::new(2))
            .build(),
    )
    .unwrap();

    assert_eq!(
        harness.quota_files().unwrap(),
        "missing=true, refused=true, freed=true"
    );
}

#[tokio::test]
async fn gated_p3_wasi_matches_plain_wasi() {
    let mut plain = P3Harness::new(false, Chain::builder().build())
        .await
        .unwrap();
    plain.basic_system().await.unwrap();
    let mut gated = P3Harness::new(true, Chain::builder().build())
        .await
        .unwrap();
    gated.basic_system().await.unwrap();

    assert_eq!(plain.stdout.contents(), gated.stdout.contents());
    assert_eq!(plain.stderr.contents(), gated.stderr.contents());
    assert_eq!(
        gated.stdout.contents().as_ref(),
        b"Hello at 1700000000.123456789\n"
    );
    assert!(gated.stderr.contents().is_empty());
}

#[tokio::test]
async fn p3_streams_and_clock_waits_retain_their_distinct_semantics() {
    let mut plain = P3Harness::new(false, Chain::builder().build())
        .await
        .unwrap();
    plain.set_monotonic_now(10_000_000_000);
    let plain_stdin = plain.exercise().await.unwrap();
    let mut gated = P3Harness::new(true, Chain::builder().build())
        .await
        .unwrap();
    gated.set_monotonic_now(10_000_000_000);
    let gated_stdin = gated.exercise().await.unwrap();

    assert!(
        plain_stdin.starts_with(concat!(
            "p3 input|random=[4, 4, 4, 4]:72623859723010820|",
            "insecure=[12, 12, 12, 12]:651345242427624204:",
            "(9843086184167632639, 4822678189205111)|",
            "16:16:true:alpha:DescriptorFlags(READ | WRITE):",
            "DescriptorType::RegularFile:true:true:",
            "[true, true, true, true, true, true, true, true, true, true, true, true, true]:true"
        )),
        "{plain_stdin}"
    );
    assert!(
        plain_stdin.contains("|sockets=true:IpAddressFamily::Ipv4:true:true:"),
        "{plain_stdin}"
    );
    assert_eq!(gated_stdin, plain_stdin);
    assert_eq!(plain.stdout.contents().as_ref(), b"p3 stdout");
    assert_eq!(gated.stdout.contents(), plain.stdout.contents());
    assert_eq!(plain.stderr.contents().as_ref(), b"p3 stderr");
    assert_eq!(gated.stderr.contents(), plain.stderr.contents());
}

#[derive(Clone, Debug)]
struct StreamCall {
    function: String,
    id: u64,
    descriptor: u32,
    bytes: usize,
    retained: usize,
    buffered: usize,
}

#[derive(Default)]
struct StreamTrace {
    openings: Vec<StreamCall>,
    chunks: Vec<StreamCall>,
}

#[derive(Clone, Default)]
struct SocketStreamTrace(SocketStreamCalls);

type SocketStreamCalls = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

impl Layer<State> for SocketStreamTrace {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        if call.interface == Some("wasi:sockets/types") && call.function.starts_with("[stream-") {
            let bytes = call
                .args
                .get("bytes")
                .and_then(ArgumentValue::as_bytes)
                .unwrap()
                .to_vec();
            self.0
                .lock()
                .unwrap()
                .push((call.function.to_owned(), bytes));
        }
        Ok(())
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

#[tokio::test]
async fn p3_tcp_stream_relay_exposes_bytes_in_both_directions() {
    let trace = SocketStreamTrace::default();
    let mut harness = P3Harness::new_relayed(Chain::builder().layer(trace.clone()).build())
        .await
        .unwrap();

    harness.exercise().await.unwrap();

    let calls = trace.0.lock().unwrap();
    assert_eq!(calls.len(), 4, "{calls:?}");
    assert_eq!(
        calls
            .iter()
            .filter(|(function, _)| function == "[stream-write]tcp-socket.send")
            .count(),
        2
    );
    assert_eq!(
        calls
            .iter()
            .filter(|(function, _)| function == "[stream-read]tcp-socket.receive")
            .count(),
        2
    );
    assert!(calls.iter().all(|(_, bytes)| bytes == b"hello"));
}

#[tokio::test(flavor = "multi_thread")]
async fn p3_tcp_send_denial_drains_every_acknowledged_byte() {
    const SIZE: usize = 4 * 1024 * 1024;

    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let peer = std::thread::spawn(move || {
        use std::io::Read as _;

        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        bytes
    });
    let chain = Chain::builder()
        .layer(Budget::new(100 * 1024, |call: &Call<'_>| {
            (call.function == "[stream-write]tcp-socket.send")
                .then(|| {
                    call.args
                        .get("bytes")
                        .and_then(ArgumentValue::byte_len)
                        .map(|bytes| ((), u64::try_from(bytes).unwrap()))
                })
                .flatten()
        }))
        .build();
    let mut harness = P3Harness::new_relayed(chain).await.unwrap();

    let (acknowledged, denied) = tokio::time::timeout(
        Duration::from_secs(10),
        harness.tcp_send_denial(port, u64::try_from(SIZE).unwrap()),
    )
    .await
    .unwrap()
    .unwrap();
    drop(harness);
    let peer_bytes = peer.join().unwrap();
    let acknowledged = usize::try_from(acknowledged).unwrap();

    assert!(denied);
    assert!(acknowledged > 0 && acknowledged < SIZE);
    assert_eq!(peer_bytes.len(), acknowledged);
    assert_eq!(peer_bytes, stream_data(acknowledged));
}

#[derive(Clone)]
struct RecordStreams(Arc<Mutex<StreamTrace>>);

impl Layer<State> for RecordStreams {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        if call.interface != Some("wasi:filesystem/types") {
            return Ok(());
        }
        let Some(descriptor) = call.handles.first().copied() else {
            return Ok(());
        };
        let mut trace = self.0.lock().unwrap();
        if call.function.starts_with("[stream-") {
            let bytes = call
                .args
                .get("bytes")
                .and_then(ArgumentValue::byte_len)
                .unwrap();
            let retained = call
                .args
                .get("bytes")
                .and_then(ArgumentValue::as_bytes)
                .map(<[u8]>::len)
                .unwrap();
            let buffered = call
                .args
                .get("buffered")
                .and_then(ArgumentValue::as_u64)
                .and_then(|value| usize::try_from(value).ok())
                .unwrap();
            trace.chunks.push(StreamCall {
                function: call.function.to_owned(),
                id: call.id,
                descriptor,
                bytes,
                retained,
                buffered,
            });
        } else if matches!(
            call.function,
            "[method]descriptor.read-via-stream"
                | "[method]descriptor.write-via-stream"
                | "[method]descriptor.append-via-stream"
        ) {
            trace.openings.push(StreamCall {
                function: call.function.to_owned(),
                id: call.id,
                descriptor,
                bytes: 0,
                retained: 0,
                buffered: 0,
            });
        }
        Ok(())
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

fn stream_data(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from((index * 31 + index / 7) & 0xff).unwrap())
        .collect()
}

fn assert_chunks_match_opening(trace: &StreamTrace, stream_name: &str, opening_name: &str) {
    let opening = trace
        .openings
        .iter()
        .find(|call| call.function == opening_name)
        .unwrap();
    assert!(
        trace
            .chunks
            .iter()
            .filter(|call| call.function == stream_name)
            .all(|chunk| chunk.id == opening.id && chunk.descriptor == opening.descriptor)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn relayed_and_unrelayed_file_reads_match() {
    let contents = stream_data(1024 * 1024 + 37);
    let mut direct = P3Harness::new(true, Chain::builder().build())
        .await
        .unwrap();
    fs::write(direct.directory.path().join("large.bin"), &contents).unwrap();
    let direct_result = direct.stream_read("large.bin", false).await.unwrap();

    let trace = Arc::new(Mutex::new(StreamTrace::default()));
    let mut relayed = P3Harness::new_relayed(
        Chain::builder()
            .layer(RecordStreams(Arc::clone(&trace)))
            .build(),
    )
    .await
    .unwrap();
    fs::write(relayed.directory.path().join("large.bin"), &contents).unwrap();
    let relayed_result = relayed.stream_read("large.bin", false).await.unwrap();

    assert_eq!(relayed_result, direct_result);
    let trace = trace.lock().unwrap();
    assert_eq!(
        trace.chunks.iter().map(|chunk| chunk.bytes).sum::<usize>(),
        contents.len()
    );
    assert!(
        trace
            .chunks
            .iter()
            .all(|chunk| chunk.retained == chunk.bytes)
    );
    assert_chunks_match_opening(
        &trace,
        "[stream-read]read-via-stream",
        "[method]descriptor.read-via-stream",
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn relayed_stdio_preserves_bytes_and_exposes_chunks() {
    let mut direct = P3Harness::new(true, Chain::builder().build())
        .await
        .unwrap();
    let direct_result = direct.exercise().await.unwrap();

    let calls = Arc::new(Mutex::new(BTreeSet::new()));
    let mut relayed =
        P3Harness::new_relayed(Chain::builder().layer(Record(Arc::clone(&calls))).build())
            .await
            .unwrap();
    let relayed_result = relayed.exercise().await.unwrap();

    assert_eq!(relayed_result, direct_result);
    assert_eq!(relayed.stdout.contents(), direct.stdout.contents());
    assert_eq!(relayed.stderr.contents(), direct.stderr.contents());
    let calls = calls.lock().unwrap();
    assert!(calls.contains(&(
        "wasi:cli/stdin".to_owned(),
        "[stream-read]read-via-stream".to_owned(),
    )));
    assert!(calls.contains(&(
        "wasi:cli/stdout".to_owned(),
        "[stream-write]write-via-stream".to_owned(),
    )));
    assert!(calls.contains(&(
        "wasi:cli/stderr".to_owned(),
        "[stream-write]write-via-stream".to_owned(),
    )));
}

#[tokio::test(flavor = "multi_thread")]
async fn slow_file_reader_never_exceeds_the_relay_bound() {
    let trace = Arc::new(Mutex::new(StreamTrace::default()));
    let mut harness = P3Harness::new_relayed(
        Chain::builder()
            .layer(RecordStreams(Arc::clone(&trace)))
            .build(),
    )
    .await
    .unwrap();
    let contents = stream_data(8 * 1024 * 1024);
    fs::write(harness.directory.path().join("slow.bin"), &contents).unwrap();

    let result = harness.stream_read("slow.bin", true).await.unwrap();

    assert!(result.contains(&format!("bytes={}", contents.len())));
    let high_water = trace
        .lock()
        .unwrap()
        .chunks
        .iter()
        .map(|chunk| chunk.buffered)
        .max()
        .unwrap();
    assert!(high_water <= wasm_component_middleware_wasi::p3::DEFAULT_STREAM_BUFFER_CAPACITY);
    assert_eq!(
        high_water,
        wasm_component_middleware_wasi::p3::DEFAULT_STREAM_BUFFER_CAPACITY
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn midstream_denial_returns_access_through_the_completion_future() {
    let chain = Chain::builder()
        .layer(Budget::new(100 * 1024, |call: &Call<'_>| {
            (call.function == "[stream-read]read-via-stream")
                .then(|| {
                    call.args
                        .get("bytes")
                        .and_then(ArgumentValue::byte_len)
                        .map(|bytes| ((), u64::try_from(bytes).unwrap()))
                })
                .flatten()
        }))
        .build();
    let mut harness = P3Harness::new_relayed(chain).await.unwrap();
    fs::write(
        harness.directory.path().join("denied.bin"),
        stream_data(1024 * 1024),
    )
    .unwrap();

    let result = tokio::time::timeout(
        Duration::from_secs(10),
        harness.stream_read("denied.bin", false),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(result.contains("completion=ErrorCode::Access"), "{result}");
}

#[tokio::test(flavor = "multi_thread")]
async fn relayed_writes_and_appends_preserve_bytes_and_origin() {
    let size = 1024 * 1024 + 91;
    let mut direct = P3Harness::new(true, Chain::builder().build())
        .await
        .unwrap();
    let direct_result = direct.stream_write("written.bin", size).await.unwrap();
    let direct_bytes = fs::read(direct.directory.path().join("written.bin")).unwrap();

    let trace = Arc::new(Mutex::new(StreamTrace::default()));
    let mut harness = P3Harness::new_relayed(
        Chain::builder()
            .layer(RecordStreams(Arc::clone(&trace)))
            .build(),
    )
    .await
    .unwrap();
    let result = harness.stream_write("written.bin", size).await.unwrap();

    assert_eq!(direct_result, "write=Ok(()), append=Ok(())");
    assert_eq!(result, direct_result);
    let bytes = fs::read(harness.directory.path().join("written.bin")).unwrap();
    assert_eq!(bytes, direct_bytes);
    assert_eq!(bytes.len(), usize::try_from(size).unwrap());
    let split = size - size / 4;
    let mut expected = stream_data(usize::try_from(split).unwrap());
    expected.extend(stream_data(usize::try_from(size - split).unwrap()));
    assert_eq!(bytes, expected);
    let trace = trace.lock().unwrap();
    assert_eq!(
        trace.chunks.iter().map(|chunk| chunk.bytes).sum::<usize>(),
        bytes.len()
    );
    assert_chunks_match_opening(
        &trace,
        "[stream-write]write-via-stream",
        "[method]descriptor.write-via-stream",
    );
    assert_chunks_match_opening(
        &trace,
        "[stream-write]append-via-stream",
        "[method]descriptor.append-via-stream",
    );
}

fn reported_count(result: &str, name: &str) -> usize {
    result
        .split(", ")
        .find_map(|field| field.strip_prefix(&format!("{name}=")))
        .unwrap()
        .parse()
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn midstream_write_denial_drains_every_acknowledged_byte() {
    const SIZE: usize = 4 * 1024 * 1024;

    let chain = Chain::builder()
        .layer(Budget::new(100 * 1024, |call: &Call<'_>| {
            (call.function == "[stream-write]write-via-stream")
                .then(|| {
                    call.args
                        .get("bytes")
                        .and_then(ArgumentValue::byte_len)
                        .map(|bytes| ((), u64::try_from(bytes).unwrap()))
                })
                .flatten()
        }))
        .build();
    let mut harness = P3Harness::new_relayed(chain).await.unwrap();

    let result = tokio::time::timeout(
        Duration::from_secs(10),
        harness.stream_write_tolerant("denied-write.bin", u64::try_from(SIZE).unwrap()),
    )
    .await
    .unwrap()
    .unwrap();

    assert!(
        result.contains("completion=Err(ErrorCode::Access)"),
        "{result}"
    );
    let offered = reported_count(&result, "offered");
    let acknowledged = reported_count(&result, "acknowledged");
    let leftover = reported_count(&result, "leftover");
    assert_eq!(offered, SIZE);
    assert_eq!(acknowledged + leftover, offered);
    let written = fs::read(harness.directory.path().join("denied-write.bin")).unwrap();
    assert_eq!(written.len(), acknowledged, "{result}");
    assert_eq!(written, stream_data(acknowledged));
}

#[tokio::test(flavor = "multi_thread")]
async fn write_side_queue_stays_at_its_bound_while_the_file_consumer_drains() {
    const SIZE: usize = 8 * 1024 * 1024;

    let trace = Arc::new(Mutex::new(StreamTrace::default()));
    let mut harness = P3Harness::new_relayed(
        Chain::builder()
            .layer(RecordStreams(Arc::clone(&trace)))
            .build(),
    )
    .await
    .unwrap();

    let result = harness
        .stream_write_tolerant("bounded-write.bin", u64::try_from(SIZE).unwrap())
        .await
        .unwrap();

    assert!(result.contains("completion=Ok(())"), "{result}");
    assert_eq!(reported_count(&result, "acknowledged"), SIZE);
    let trace = trace.lock().unwrap();
    let write_chunks = trace
        .chunks
        .iter()
        .filter(|chunk| chunk.function == "[stream-write]write-via-stream")
        .collect::<Vec<_>>();
    assert!(
        write_chunks
            .iter()
            .all(|chunk| chunk.retained == chunk.bytes)
    );
    let high_water = write_chunks
        .into_iter()
        .map(|chunk| chunk.buffered)
        .max()
        .unwrap();
    assert_eq!(
        high_water,
        wasm_component_middleware_wasi::p3::DEFAULT_STREAM_BUFFER_CAPACITY
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_relayed_read_does_not_hang() {
    let mut harness = P3Harness::new_relayed(Chain::builder().build())
        .await
        .unwrap();
    fs::write(
        harness.directory.path().join("cancel.bin"),
        stream_data(8 * 1024 * 1024),
    )
    .unwrap();

    tokio::time::timeout(
        Duration::from_secs(10),
        harness.cancel_stream_read("cancel.bin"),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn cancelling_a_relayed_write_does_not_hang() {
    let mut harness = P3Harness::new_relayed(Chain::builder().build())
        .await
        .unwrap();

    tokio::time::timeout(
        Duration::from_secs(10),
        harness.cancel_stream_write("cancel-write.bin"),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "throughput benchmark; run in release mode with --ignored --nocapture"]
async fn relay_throughput() {
    const SIZE: usize = 64 * 1024 * 1024;
    const RUNS: usize = 3;

    for (path, blocking) in [("buffered", false), ("direct", true)] {
        for (mode, relayed) in [("unrelayed", false), ("relayed", true)] {
            let mut harness = P3Harness::new_for_benchmark(relayed, blocking)
                .await
                .unwrap();
            fs::write(
                harness.directory.path().join("benchmark.bin"),
                stream_data(SIZE),
            )
            .unwrap();
            harness.stream_read("benchmark.bin", false).await.unwrap();
            let started = Instant::now();
            for _ in 0..RUNS {
                let result = harness.stream_read("benchmark.bin", false).await.unwrap();
                assert!(result.contains("completion=ok"));
            }
            let mib_per_second = f64::from(u32::try_from(SIZE * RUNS).unwrap())
                / (1024.0 * 1024.0)
                / started.elapsed().as_secs_f64();
            println!("{path:8} {mode:9} {mib_per_second:8.2} MiB/s");
        }
    }
}

#[derive(Clone)]
struct Sequence(Arc<Mutex<Vec<(String, String)>>>);

impl Layer<State> for Sequence {
    type Frame = ();

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<(), Denied> {
        if let Some(interface) = call.interface {
            self.0
                .lock()
                .unwrap()
                .push((interface.to_owned(), call.function.to_owned()));
        }
        Ok(())
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

#[tokio::test]
async fn one_layer_records_the_same_shared_calls_for_p2_and_p3() {
    let p2_calls = Arc::new(Mutex::new(Vec::new()));
    let mut p2 = Harness::new(
        true,
        Chain::builder()
            .layer(Sequence(Arc::clone(&p2_calls)))
            .build(),
    )
    .unwrap();
    p2.guest.call_common_calls(&mut p2.store).unwrap();

    let p3_calls = Arc::new(Mutex::new(Vec::new()));
    let mut p3 = P3Harness::new(
        true,
        Chain::builder()
            .layer(Sequence(Arc::clone(&p3_calls)))
            .build(),
    )
    .await
    .unwrap();
    p3.common_calls().await.unwrap();

    assert_eq!(*p2_calls.lock().unwrap(), *p3_calls.lock().unwrap());
}

#[derive(Clone)]
struct CancellationCount(Arc<Mutex<usize>>);

impl Layer<State> for CancellationCount {
    type Frame = bool;

    fn before(&self, _state: &mut State, call: &Call<'_>) -> Result<bool, Denied> {
        Ok(call.function == "wait-for")
    }

    fn after(&self, _state: &mut State, _call: &Call<'_>, tracked: bool, outcome: Outcome<'_>) {
        if tracked && matches!(outcome, Outcome::Cancelled) {
            *self.0.lock().unwrap() += 1;
        }
    }
}

#[tokio::test]
async fn cancelled_p3_call_reports_once() {
    let cancellations = Arc::new(Mutex::new(0));
    let mut harness = P3Harness::new(
        true,
        Chain::builder()
            .layer(CancellationCount(Arc::clone(&cancellations)))
            .build(),
    )
    .await
    .unwrap();

    harness.cancel_wait().await.unwrap();

    assert_eq!(*cancellations.lock().unwrap(), 1);
}

#[tokio::test]
async fn dropping_a_store_with_a_pending_p3_call_does_not_panic() {
    let mut harness = P3Harness::new(true, Chain::builder().build())
        .await
        .unwrap();

    harness.abandon_wait().await.unwrap();
    drop(harness);
}

#[test]
fn run_example_traces_preview_2_and_preview_3_commands() {
    let directory = tempfile::tempdir().unwrap();
    let contents = "middleware kept this output\n".repeat(200);
    std::fs::write(directory.path().join("note.txt"), &contents).unwrap();
    let guests = [
        (
            guest_build::cat_p2(),
            "wasi:io/streams@0.2.12.[method]output-stream.blocking-write-and-flush",
        ),
        (
            guest_build::cat_p3(),
            "wasi:cli/stdout@0.3.0.write-via-stream",
        ),
    ];

    for (guest, expected_call) in guests {
        let output = guest_build::run_example_with_args(
            "wasm-component-middleware-wasi",
            "run",
            &[guest.to_str().unwrap(), "note.txt"],
            directory.path(),
        )
        .unwrap();

        guest_build::assert_example_succeeded(&output);
        assert_eq!(String::from_utf8(output.stdout).unwrap(), contents);
        let trace = String::from_utf8(output.stderr).unwrap();
        assert!(trace.contains(expected_call), "{trace}");
        assert!(trace.contains("wasi:filesystem/types"), "{trace}");
    }
}

#[test]
fn run_example_propagates_guest_exit_codes() {
    for (code, success) in [(0, true), (3, false)] {
        let argument = format!("--exit={code}");
        let output = guest_build::run_example_with_args(
            "wasm-component-middleware-wasi",
            "run",
            &[guest_build::cat_p2().to_str().unwrap(), &argument],
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).as_path(),
        )
        .unwrap();

        assert_eq!(output.status.success(), success);
        assert_eq!(output.status.code(), Some(code));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(!stderr.contains("wasm backtrace"), "{stderr}");
    }
}

#[tokio::test]
async fn command_guests_match_plain_wasi() {
    for (guest, preview_3) in [
        (guest_build::cat_p2(), false),
        (guest_build::cat_p3(), true),
    ] {
        let plain = run_command_guest(guest, preview_3, false).await.unwrap();
        let gated = run_command_guest(guest, preview_3, true).await.unwrap();

        assert_eq!(gated, plain);
        assert_eq!(plain, b"differential output\n");
    }
}

#[test]
fn sandbox_guest_matches_plain_wasi() {
    let plain = run_sandbox_guest(false).unwrap();
    let gated = run_sandbox_guest(true).unwrap();

    assert_eq!(gated, plain);
}

fn run_sandbox_guest(gated: bool) -> wasmtime::Result<Vec<u8>> {
    let directory = tempfile::tempdir()?;
    let public = directory.path().join("public");
    let private = directory.path().join("private");
    fs::create_dir(&public)?;
    fs::create_dir(&private)?;
    for (name, contents) in [
        ("note.txt", "hello"),
        ("one.txt", "one"),
        ("two.txt", "two"),
        ("three.txt", "three"),
    ] {
        fs::write(public.join(name), contents)?;
    }
    fs::write(private.join("secret.txt"), "classified")?;

    let engine = Engine::default();
    let component = Component::from_file(&engine, guest_build::sandbox())?;
    let mut linker = Linker::new(&engine);
    if gated {
        wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker)?;
    } else {
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    }

    let stdout = MemoryOutputPipe::new(4096);
    let mut wasi = WasiCtxBuilder::new();
    wasi.stdout(stdout.clone())
        .preopened_dir(&public, "public", FsPerms::ReadWrite)?
        .preopened_dir(&private, "private", FsPerms::ReadWrite)?;
    let mut store = Store::new(
        &engine,
        State {
            middleware: Some(MiddlewareCtx::new(
                Chain::builder().build(),
                InvocationContext::new("differential"),
            )),
            table: ResourceTable::new(),
            wasi: wasi.build(),
        },
    );
    let guest = sandbox_guest::Sandbox::instantiate(&mut store, &component, &linker)?;
    guest.call_run(&mut store)?;
    Ok(stdout.contents().to_vec())
}

async fn run_command_guest(
    guest: &std::path::Path,
    preview_3: bool,
    gated: bool,
) -> wasmtime::Result<Vec<u8>> {
    let mut config = Config::new();
    config.wasm_component_model_async(true);
    config.concurrency_support(true);
    let engine = Engine::new(&config)?;
    let component = Component::from_file(&engine, guest)?;
    let mut linker = Linker::new(&engine);
    if gated {
        wasm_component_middleware_wasi::p2::add_to_linker_async(&mut linker)?;
    } else {
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
    }
    if preview_3 {
        if gated {
            wasm_component_middleware_wasi::p3::add_to_linker(&mut linker)?;
        } else {
            wasmtime_wasi::p3::add_to_linker(&mut linker)?;
        }
    }

    let directory = tempfile::tempdir()?;
    fs::write(directory.path().join("note.txt"), b"differential output\n")?;
    let stdout = MemoryOutputPipe::new(4096);
    let mut wasi = WasiCtxBuilder::new();
    wasi.args(&["cat", "note.txt"])
        .stdout(stdout.clone())
        .preopened_dir(directory.path(), ".", FsPerms::ReadOnly)?;
    let chain = Chain::builder().build();
    let mut store = Store::new(
        &engine,
        State {
            middleware: Some(MiddlewareCtx::new(
                chain,
                InvocationContext::new("differential"),
            )),
            table: ResourceTable::new(),
            wasi: wasi.build(),
        },
    );

    if preview_3 {
        let command = wasmtime_wasi::p3::bindings::Command::instantiate_async(
            &mut store, &component, &linker,
        )
        .await?;
        let result = store
            .run_concurrent(async move |accessor| command.wasi_cli_run().call_run(accessor).await)
            .await??;
        result.map_err(|()| wasmtime::Error::msg("command returned failure"))?;
    } else {
        let command = wasmtime_wasi::p2::bindings::Command::instantiate_async(
            &mut store, &component, &linker,
        )
        .await?;
        command
            .wasi_cli_run()
            .call_run(&mut store)
            .await?
            .map_err(|()| wasmtime::Error::msg("command returned failure"))?;
    }

    Ok(stdout.contents().to_vec())
}
