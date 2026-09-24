# HTTP allowlist

This example checks complete outgoing HTTP requests at the send hook. It starts
two loopback servers, allows one authority, and refuses the other.

Run it from the repository root:

```console
cargo run -p http-allowlist
```

Ports vary, but the result has this shape:

```text
allowed body: GET http://127.0.0.1:<port>/message ... | hello from the allowed server
denied error: ErrorCode::HttpRequestDenied
```
