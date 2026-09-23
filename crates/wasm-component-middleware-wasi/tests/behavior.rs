#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::future::poll_fn;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWrite};
use wasm_component_middleware::{
    Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx, MiddlewareView, Outcome,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::cli::{IsTerminal, StdinStream, StdoutStream};
use wasmtime_wasi::p2::pipe::{MemoryInputPipe, MemoryOutputPipe};
use wasmtime_wasi::p2::{InputStream, OutputStream, Pollable, StreamResult};
use wasmtime_wasi::{
    HostMonotonicClock, HostWallClock, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView,
};
use wit_parser::{Resolve, TypeDefKind};

wasmtime::component::bindgen!({
    path: "../../guests/wasi-p2/wit",
    world: "workload",
});

mod p3 {
    wasmtime::component::bindgen!({
        path: "../../guests/wasi-p3/wit",
        world: "workload",
        imports: { default: async | store },
        exports: { default: async | store },
        with: { "wasi": wasmtime_wasi::p3::bindings },
        require_store_data_send: true,
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
}

struct P3Harness {
    store: Store<State>,
    guest: p3::Workload,
    stdout: MemoryOutputPipe,
    stderr: MemoryOutputPipe,
    monotonic_now: Arc<AtomicU64>,
}

impl P3Harness {
    async fn new(gated: bool, chain: Arc<Chain<State>>) -> wasmtime::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model_async(true);
        config.concurrency_support(true);
        let engine = Engine::new(&config)?;
        let component = Component::from_file(&engine, test_guests::wasi_p3())?;
        let mut linker = Linker::new(&engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        if gated {
            wasm_component_middleware_wasi::p3::add_to_linker(&mut linker)?;
        } else {
            wasmtime_wasi::p3::add_to_linker(&mut linker)?;
        }

        let stdout = MemoryOutputPipe::new(4096);
        let stderr = MemoryOutputPipe::new(4096);
        let monotonic_now = Arc::new(AtomicU64::new(10_000_000_000));
        let mut builder = WasiCtxBuilder::new();
        builder
            .env("GREETING", "Hello")
            .args(&["program", "one"])
            .stdin(MemoryInputPipe::new("p3 input"))
            .stdout(stdout.clone())
            .stderr(stderr.clone())
            .wall_clock(FixedWallClock)
            .monotonic_clock(ProgrammableMonotonicClock(Arc::clone(&monotonic_now)));
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
}

impl Harness {
    fn new(gated: bool, chain: Arc<Chain<State>>) -> wasmtime::Result<Self> {
        let engine = Engine::default();
        let component = Component::from_file(&engine, test_guests::wasi_p2())?;
        let mut linker = Linker::new(&engine);
        if gated {
            wasm_component_middleware_wasi::p2::add_to_linker_sync(&mut linker)?;
        } else {
            wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        }

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
            .wall_clock(FixedWallClock)
            .monotonic_clock(FixedMonotonicClock);
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
        let guest = Workload::instantiate(&mut store, &component, &linker)?;
        Ok(Self {
            store,
            guest,
            stdout,
            stderr,
            io,
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
    for function in ["get-environment", "write-via-stream"] {
        let mut harness = P3Harness::new(true, Chain::builder().layer(Refuse(function)).build())
            .await
            .unwrap();
        let error = harness.basic_system().await.unwrap_err();

        assert_eq!(
            error.downcast_ref::<Denied>().unwrap().reason(),
            format!("{function} is disabled")
        );
    }
}

#[test]
fn gated_wasi_matches_plain_wasi() {
    assert_eq!(run(false).unwrap(), run(true).unwrap());
    assert_eq!(run_exercise(false).unwrap(), run_exercise(true).unwrap());
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

    assert_eq!(plain_stdin, "p3 input");
    assert_eq!(gated_stdin, plain_stdin);
    assert_eq!(plain.stdout.contents().as_ref(), b"p3 stdout");
    assert_eq!(gated.stdout.contents(), plain.stdout.contents());
    assert_eq!(plain.stderr.contents().as_ref(), b"p3 stderr");
    assert_eq!(gated.stderr.contents(), plain.stderr.contents());
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
fn wasi_example_prints_the_trace_and_guest_output() {
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "-p",
            "wasm-component-middleware-wasi",
            "--example",
            "wasi-p2",
            "--locked",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Hello from WASI at 1700000000.123456789\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        concat!(
            "→ #1 import wasi:cli/environment@0.2.12.get-environment()\n",
            "← #1 returned\n",
            "→ #2 import wasi:clocks/wall-clock@0.2.12.now()\n",
            "← #2 returned\n",
            "→ #3 import wasi:cli/stdout@0.2.12.get-stdout()\n",
            "← #3 returned produced=[0]\n",
            "→ #4 import wasi:io/streams@0.2.12.[method]output-stream.blocking-write-and-flush(bytes=40 bytes \"Hello from WASI at 17000…\")\n",
            "← #4 returned\n",
            "→ #5 import wasi:io/streams@0.2.12.[resource-drop]output-stream()\n",
            "← #5 returned\n",
        )
    );
}

#[test]
fn wasi_p3_example_prints_the_trace_and_guest_output() {
    let output = Command::new(env!("CARGO"))
        .args([
            "run",
            "--quiet",
            "-p",
            "wasm-component-middleware-wasi",
            "--example",
            "wasi-p3",
            "--locked",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Hello from WASI at 1700000000.123456789\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        concat!(
            "→ #1 import wasi:cli/environment@0.3.0.get-environment()\n",
            "← #1 returned\n",
            "→ #2 import wasi:clocks/system-clock@0.3.0.now()\n",
            "← #2 returned\n",
            "→ #3 import wasi:cli/stdout@0.3.0.write-via-stream()\n",
            "← #3 returned\n",
        )
    );
}
