# Deny

This example combines logging with an allowlist that first refuses the host
user-name import, then allows the complete interface in a fresh store.

Run it from the repository root:

```console
cargo run -p deny
```

The output shows the refusal followed by a successful greeting:

```text
denied:
...
greet failed: import example:hello/host.user-name is not allowed
allowed:
...
Hello, Ada!
```
