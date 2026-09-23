# wasm-component-middleware

`wasm-component-middleware` lets a Wasmtime host put policy and observability
layers around WebAssembly component calls. A single chain can observe or deny
the host's imports and the component's exports while preserving their nesting.

This project is under construction.

The [`trace` example](crates/wasm-component-middleware/examples/trace.rs) builds
a chain with the included logger:

```rust
let chain = Chain::builder().layer(Logger::stderr()).build();
let mut store = Store::new(
    &engine,
    State {
        middleware: MiddlewareCtx::new(chain, InvocationContext::new("hello")),
        // application state
    },
);
```

Generated host imports are linked through `Routed<State>`. Their ordinary
`Host for State` implementation remains the application's source of behavior;
`route_imports!` generates the routing adapter. Bindings used this way must
enable trappable imports so middleware can refuse a call:

```rust
wasmtime::component::bindgen!({
    path: "../../guests/hello/wit",
    world: "hello",
    imports: { default: trappable },
});

route_imports! {
    const HELLO_HOST: example::hello::host::Host => State as "example:hello/host" {
        fn user_name(&mut self) -> wasmtime::Result<String>;
        fn log(&mut self, message: String) -> wasmtime::Result<()>;
    }
}

example::hello::host::add_to_linker::<_, Routed<State>>(
    &mut linker,
    Routed::<State>::get,
)?;
verify_routing(
    &engine,
    &component,
    [HELLO_HOST],
    ["wasi:"],
)?;
```

If middleware refuses an imported function whose WIT result has no error case,
Wasmtime reports the refusal to the guest as a trap.

Run it from the repository root:

```console
$ cargo run --example trace
→ #1 export greet(greeting="Hello")
  → #2 import example:hello/host.user-name()
  ← #2 returned
  → #3 import example:hello/host.log(message="greeting Ada")
  ← #3 returned
← #1 returned
Hello, Ada!
```

## WASI

Switch synchronous WASI Preview 2 calls into the same chain by changing the
import used to populate the linker:

```rust
// use wasmtime_wasi::p2::add_to_linker_sync;
use wasm_component_middleware_wasi::p2::add_to_linker_sync;

add_to_linker_sync(&mut linker)?;
```

The middleware linker routes `wasi:cli`, `wasi:clocks`, `wasi:filesystem`,
and `wasi:random` on both previews, plus `wasi:io` and `wasi:sockets` on
Preview 2.

The [`wasi-p2` example](crates/wasm-component-middleware-wasi/examples/wasi-p2.rs)
runs a Rust guest that reads an environment variable and the wall clock before
writing to standard output:

```console
$ cargo run --example wasi-p2
→ #1 import wasi:cli/environment@0.2.12.get-environment()
← #1 returned
→ #2 import wasi:clocks/wall-clock@0.2.12.now()
← #2 returned
...
Hello from WASI at 1700000000.123456789
```

Preview 3 uses the same one-line switch:

```rust
// use wasmtime_wasi::p3::add_to_linker;
use wasm_component_middleware_wasi::p3::add_to_linker;

add_to_linker(&mut linker)?;
```

Rust Preview 3 components still import Preview 2 through the standard library,
so link `wasmtime_wasi::p2::add_to_linker_async` as well. The
[`wasi-p3` example](crates/wasm-component-middleware-wasi/examples/wasi-p3.rs)
shows both linker calls and the concurrent export invocation. A refusal of
filesystem `read-via-stream`, `write-via-stream`, `append-via-stream`, or
`read-directory` traps because those Preview 3 calls have no top-level error
result.

## Streams

Preview 3 moves file bytes through Component Model streams rather than host
calls. The default `p3::add_to_linker` gates the call that opens each stream but
leaves its bytes on Wasmtime's direct path. This also preserves Wasmtime's
`try_into` short circuit for host-to-host streams.

Use the opt-in relay when middleware must inspect or restrict every chunk:

```rust
use wasm_component_middleware_wasi::p3::{
    StreamRelay, add_to_linker_with_stream_relay,
};

add_to_linker_with_stream_relay(&mut linker, StreamRelay::default())?;
```

The default queue bound is 64 KiB. A different nonzero bound can be selected
with a const generic, for example `StreamRelay::<8192>::new()`. Relayed chunks
appear as `[stream-read]read-via-stream`,
`[stream-write]write-via-stream`, or
`[stream-write]append-via-stream` calls. They reuse the opening call's id and
descriptor handle; `args["bytes"]` exposes the complete chunk because relay
data has already been copied. Ordinary gate argument snapshots retain the
64-byte cap. Refusing a chunk leaves it unacknowledged, drains all
previously approved chunks, and resolves the companion future as
`error-code::access`, so the guest sees a recoverable filesystem error.
The [`byte-budget` example](crates/wasm-component-middleware-wasi/examples/byte-budget.rs)
uses this path to share a 100 KiB read allowance across stores.

Relaying copies bytes through a bounded host queue. On the direct filesystem
path this can reduce throughput by about 40%; the buffered path is usually much
closer. The ignored `relay_throughput` test measures both paths over a 64 MiB
file in a release build.

## Files

Filesystem gates expose paths through `Call::args` and report every descriptor
in `Completion::produced`, including preopened directories. `OpenFiles` uses
those descriptors and their resource-drop calls to enforce a per-invocation
limit:

```rust
let chain = Chain::builder()
    .layer(Logger::stderr())
    .layer(OpenFiles::new(4))
    .layer(RefusePrivate)
    .build();
```

The path policy in the [`sandbox` example](crates/wasm-component-middleware-wasi/examples/sandbox.rs)
labels the descriptors returned for a private preopen, propagates that label
through descriptors opened beneath it, and removes labels when descriptors are
dropped. It refuses path-taking calls based on their handles. Matching path
strings is incorrect because middleware does not resolve `..` or symlinks the
way wasmtime-wasi does. Both the descriptor policy and limit become
`error-code::access` results visible to the guest:

```console
$ cargo run --example sandbox
...
read public/note.txt: hello
read private/secret.txt: access
escape from public preopen: access
open public/one.txt: allowed
open public/two.txt: allowed
open public/three.txt: access
```

## Sockets

Preview 2 socket gates expose bind, connect, UDP stream, and datagram
destinations through `Call::args`. An `ip-socket-address` is rendered as a
standard `host:port` string, so a layer can use `SocketAddr` instead of
reimplementing the WIT variants. The [`net-allowlist` example](crates/wasm-component-middleware-wasi/examples/net-allowlist.rs)
allows one loopback destination and refuses another. Its UDP policy checks
both the optional address passed to `udp-socket.stream` and every remote
address passed to `outgoing-datagram-stream.send`:

```rust
let allowed = call
    .args
    .get("remote_address")
    .and_then(ArgumentValue::as_str)
    .and_then(|address| address.parse::<SocketAddr>().ok())
    .is_some_and(|address| address == self.0);
if allowed {
    Ok(())
} else {
    Err(Denied::new("remote address is not allowed"))
}
```

```console
$ cargo run --example net-allowlist
...
→ #12 import wasi:sockets/tcp@0.2.12.[method]tcp-socket.start-connect(remote_address="127.0.0.1:62502") handles=[1, 0]
← #12 returned
...
→ #25 import wasi:sockets/tcp@0.2.12.[method]tcp-socket.start-connect(remote_address="127.0.0.1:62503") handles=[1, 0]
← #25 failed: remote address is not allowed
...
→ #34 import wasi:sockets/udp@0.2.12.[method]outgoing-datagram-stream.send(datagrams=[[3 bytes "udp", some("127.0.0.1:62503")]]) handles=[3]
← #34 failed: remote address is not allowed
...
→ #45 import wasi:sockets/udp@0.2.12.[method]outgoing-datagram-stream.send(datagrams=[[3 bytes "udp", some("127.0.0.1:62502")]]) handles=[2]
← #45 returned
allowed: hello
denied: access-denied
udp allowed: delivered
udp denied: access-denied
```

If a component must never use sockets, do not link the socket interfaces.
That is cheaper and less error-prone than gating their large API surface.

## Refusing calls

The [`deny` example](crates/wasm-component-middleware/examples/deny.rs) puts a
logger outside an allowlist that permits the root export and `log`, but refuses
`user-name`:

```rust
let calls = Allowlist::new()
    .allow_function(None, "greet")
    .allow_function(Some(HELLO_HOST.name()), "log");
let chain = Chain::builder()
    .layer(Logger::stderr())
    .layer(calls)
    .build();
```

For an imported function whose WIT result has no error case, a `Denied` error
becomes a trap and the trapped store cannot be entered again. If refusal is an
expected guest-visible outcome, model it in WIT as a `result` and map the
denial into its error variant instead of propagating a Wasmtime error.
