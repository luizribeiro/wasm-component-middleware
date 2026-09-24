# WASI Preview 2

This example routes synchronous WASI Preview 2 environment, wall-clock, and
standard-output calls through the logging layer.

Run it from the repository root:

```console
cargo run -p wasi-p2
```

Expected output includes the WASI calls and the deterministic guest line:

```text
→ #1 import wasi:cli/environment@0.2.12.get-environment()
...
Hello from WASI at 1700000000.123456789
```
