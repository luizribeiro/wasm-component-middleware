# Network allowlist

This example applies one destination policy to Preview 2 and Preview 3 TCP and
UDP sockets, allowing one loopback endpoint and refusing another.

Run it from the repository root:

```console
cargo run -p net-allowlist
```

The output reports the same policy result for both previews:

```text
p2:
allowed: hello
denied: access-denied
...
p3:
allowed: hello
denied: access-denied
...
```
