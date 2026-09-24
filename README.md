# wasm-component-middleware

`wasm-component-middleware` lets a Wasmtime host put policy and observability
layers around WebAssembly component calls. A single chain can observe or deny
the host's imports and the component's exports while preserving their nesting.

The [examples index](examples/README.md) is the quickest way to see each feature
in a complete, runnable host.

## Core middleware

Build a chain and store it with the host state:

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
verify_routing(&engine, &component, [HELLO_HOST], ["wasi:"])?;
```

Layers run outside-in before a call and inside-out afterward. They can share
state across stores through `Arc`, or keep invocation-local state in
`InvocationContext`. The core crate includes `Logger`, `Allowlist`, and
`Budget`; WASI adds `OpenFiles`. Implement `Layer<State>` for application
policy that needs typed arguments, resource handles, returned handles,
failures, or cancellation.

See [trace](examples/trace/README.md) for nested call logging and
[deny](examples/deny/README.md) for function allowlisting.

## WASI

Switch synchronous WASI Preview 2 calls into the same chain by changing the
function used to populate the linker:

```rust
use wasm_component_middleware_wasi::p2::add_to_linker_sync;

add_to_linker_sync(&mut linker)?;
```

Async Preview 2 uses `p2::add_to_linker_async`. Preview 3 uses
`p3::add_to_linker`, normally alongside the async Preview 2 linker because
Rust Preview 3 components still use Preview 2 imports through the standard
library. After registering WASI and application imports, use the WASI-aware
strict check:

```rust
wasm_component_middleware_wasi::verify_routing(
    &engine,
    &component,
    [HELLO_HOST],
)?;
```

The [Preview 2](examples/wasi-p2/README.md) and
[Preview 3](examples/wasi-p3/README.md) examples show both linker styles. The
[command runner](examples/run/README.md) accepts either command-component
version.

## Streams

Preview 3 moves filesystem, socket, and standard-I/O bytes through Component
Model streams. The default `p3::add_to_linker` gates the call that opens each
stream but leaves its bytes on Wasmtime's direct path.

Use the opt-in relay when middleware must inspect or restrict every chunk:

```rust
use wasm_component_middleware_wasi::p3::{
    StreamRelay, add_to_linker_with_stream_relay,
};

add_to_linker_with_stream_relay(&mut linker, StreamRelay::default())?;
```

The default queue bound is 64 KiB. A different nonzero bound can be selected
with a const generic, such as `StreamRelay::<8192>::new()`. Relayed chunks
appear as `[stream-read]` or `[stream-write]` calls and expose their complete
bytes through `Call::args`. Relaying copies bytes through a bounded host queue,
so it is opt-in. The [byte-budget example](examples/byte-budget/README.md)
shares one allowance across relayed reads in two stores.

## Files and sockets

Filesystem gates expose paths through `Call::args` and descriptors through
`Completion::produced`. `OpenFiles` tracks descriptor creation and resource
drops to enforce a per-invocation limit. Policies should follow descriptor
identity rather than trying to resolve path strings like Wasmtime does. The
[sandbox example](examples/sandbox/README.md) combines both techniques.

Socket gates expose bind, connect, stream, and datagram destinations as
standard `host:port` strings. Preview 3 accepted TCP sockets are created inside
a returned resource stream and therefore are not individually visible to
layers. The [network allowlist](examples/net-allowlist/README.md) applies one
policy to both WASI previews.

If a component must never use a capability, omit its interfaces from the
linker. That is cheaper and less error-prone than gating every call.

## HTTP

Use the matching module in `wasm-component-middleware-wasi-http` instead of the
corresponding `wasmtime-wasi-http` linker function. Per-function gates observe
`wasi:http` calls. To inspect a complete request, install `WasiHttpHooks`
beside `WasiHttpCtx`; it routes one synthetic
`wasi:http/request-hook.[send-request]` call with the method, scheme,
authority, path, and headers.

```rust
let policy = HttpPolicy {
    middleware: MiddlewareCtx::new(policy_chain, InvocationContext::new("http-client")),
};
let hooks = WasiHttpHooks::new(policy, DefaultHooks);
```

The hook's chain is separate from the store chain. Put the same `Arc`-backed
layer in both when they need shared state. An HTTP policy is still required
when sockets are restricted because Wasmtime's default sender opens its own
connections. See the [HTTP allowlist example](examples/http-allowlist/README.md).

## Composition and limitations

This library runs middleware in the Wasmtime host. Composition-time tools can
still wrap the component graph independently, and the two approaches can be
used together.

Application bindings must enable Wasmtime's `trappable` imports for a layer to
refuse a call. WASI gates are tied to the generated traits in one Wasmtime
release, so this workspace pins Wasmtime 49. Preview 3 byte relaying is opt-in,
Preview 3 accepted TCP sockets are not individually visible, and Preview 3 HTTP
body streams currently pass through without a byte relay.
