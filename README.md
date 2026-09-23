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
shows both linker calls and the concurrent export invocation.

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
