#![allow(missing_docs)]

use std::collections::BTreeSet;
use std::future::poll_fn;
use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::task::Poll;
use std::time::Duration;

use bytes::Bytes;
use tokio::io::{AsyncRead, AsyncWrite};
use wasm_component_middleware::{
    Call, Chain, Denied, InvocationContext, Layer, MiddlewareCtx, MiddlewareView, Outcome,
};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
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

impl Harness {
    fn new(gated: bool, chain: Chain<State>) -> wasmtime::Result<Self> {
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
                    Arc::new(chain),
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

#[test]
fn gated_wasi_matches_plain_wasi() {
    assert_eq!(run(false).unwrap(), run(true).unwrap());
    assert_eq!(run_exercise(false).unwrap(), run_exercise(true).unwrap());
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
            "← #3 returned\n",
            "→ #4 import wasi:io/streams@0.2.12.[method]output-stream.blocking-write-and-flush(([72, 101, 108, 108, 111, 32, 102, 114, 111, 109, 32, 87, 65, 83, 73, 32, 97, 116, 32, 49, 55, 48, 48, 48, 48, 48, 48, 48, 48, 46, 49, 50, 51, 52, 53, 54, 55, 56, 57, 10],))\n",
            "← #4 returned\n",
            "→ #5 import wasi:io/streams@0.2.12.[resource-drop]output-stream()\n",
            "← #5 returned\n",
        )
    );
}
