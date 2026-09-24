# Run a WASI command

This example runs and traces any Preview 2 or Preview 3 `wasi:cli/command`
component while preopening the current directory read-only.

Run it from the repository root:

```console
cargo run -p run -- path/to/component.wasm [args...]
```

Pass `--inherit-env` before the component path to expose the host environment.
The example's own small command guests are used by its output tests to verify
both preview versions and exit-code propagation.
