# WASI Preview 3

This example routes concurrent WASI Preview 3 environment, clock, and output
calls through the logging layer.

Run it from the repository root:

```console
cargo run -p wasi-p3
```

Expected output includes the Preview 3 calls and the deterministic guest line:

```text
→ #1 import wasi:cli/environment@0.3.0.get-environment()
...
Hello from WASI at 1700000000.123456789
```
