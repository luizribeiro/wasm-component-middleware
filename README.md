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

## Alternatives

Wasmtime has no built-in way to run code around a specific host call and see
its arguments, so each project that needs one picks a workaround. These are the
ones we considered, and why this crate works the way it does.

### Composing components

[splicer](https://github.com/ejrgilbert/splicer) inserts middleware components
into the edges of a component graph
([write-up](https://bytecodealliance.org/articles/how-wasm-components-enable-pluggable-middleware)).
Its middleware ships with the component and runs in any component runtime. It
can only wrap calls between components, though, and WASI and other host imports
come from the host, so they are out of reach. Refusing a call that returns a
value traps the guest, and changing which middleware runs means splicing the
binary again.

[WASI-Virt](https://github.com/bytecodealliance/WASI-Virt) reaches WASI by
composing a component that implements it in the host's place, either inside the
binary or by passing selected calls through to the host. A wrapper built that
way could call back into the host before forwarding each call. That reaches
WASI, but the host still needs a hook for every interface, so the host code does
not go away. The wrapper also has to re-wrap every file and stream handle, and
each call crosses an extra component boundary. What it gains is portability
across runtimes.

### Inside Wasmtime

- **Wrap wasmtime-wasi's handlers.** Once `add_to_linker` has run, `Linker`
  cannot hand a definition back, so there is nothing to wrap.
- **`Store::call_hook`.** It runs on every transition between guest and host
  but does not say which function was called or with what arguments. It can
  count calls, not judge them.
- **Grant less.** A narrower `WasiCtx`, or leaving an interface unlinked, is
  fixed before the component runs, so it cannot decide call by call. Use it
  alongside this crate, as [Files and sockets](#files-and-sockets) suggests.
- **Untyped dispatch.** `func_new` with `Val` can route every import in one
  loop, as
  [wasm-component-trampoline](https://github.com/andyl-technologies/wasm-component-trampoline)
  does between components. It gives up compile-time type checking, and
  wasmtime-wasi's implementation is only reachable through typed traits.
- **Fork Wasmtime.**
  [masters-wasi-security](https://github.com/idlab-discover/masters-wasi-security)
  gates every host function with a YAML or Rego policy by patching Wasmtime.
  That works, at the cost of maintaining a fork.
- **Wait for upstream.** Wasmtime has no interposition hook. The closest
  proposal, a `Linker` pre-hook discussed in
  [wasmtime#4018](https://github.com/bytecodealliance/wasmtime/issues/4018),
  would see function names but not arguments.

### Hosts that already do this

[Golem](https://github.com/golemcloud/golem) wraps every WASI function to make
it durable, [Spin](https://github.com/spinframework/spin) gates sockets and
outbound HTTP, and [wasmCloud](https://github.com/wasmCloud/wasmCloud) links
WASI one interface at a time. Each is built for one purpose inside its own
runtime, and none offers a general chain to other hosts.

### How this crate works

Every `add_to_linker` function generated by wasmtime-wasi takes a type
parameter naming what implements the interface. This crate supplies its own
type, which runs the chain around a call to wasmtime-wasi's real
implementation. WASI behavior stays Wasmtime's own, and each layer sees typed arguments, the
resource handles a call took and returned, and handle drops.

## Limitations

Middleware runs only in Wasmtime hosts. For middleware that should travel with
a component or sit between components, use a composition tool such as splicer;
the two can be combined.

Application bindings must enable Wasmtime's `trappable` imports for a layer to
refuse a call. WASI gates are written against the traits generated for one
Wasmtime release, because those signatures come from wasmtime-wasi's `bindgen!`
settings rather than the WIT, so this workspace pins Wasmtime 49.
Preview 3 byte relaying is opt-in, Preview 3 accepted TCP sockets are not
individually visible, and Preview 3 HTTP body streams currently pass through
without a byte relay.
