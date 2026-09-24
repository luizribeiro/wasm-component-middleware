# Trace

This example routes a component export and its nested host imports through a
logging layer.

Run it from the repository root:

```console
cargo run -p trace
```

The trace is written to standard error and the greeting to standard output:

```text
→ #1 export greet(greeting="Hello")
  → #2 import example:hello/host.user-name()
  ← #2 returned
  ...
← #1 returned
Hello, Ada!
```
