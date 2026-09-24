# Byte budget

This example relays Preview 3 filesystem stream chunks through middleware and
shares a 100 KiB byte budget across two stores.

Run it from the repository root:

```console
cargo run -p byte-budget
```

The first read succeeds and the second exhausts the shared allowance:

```text
stream read: 106496 bytes in 13 chunks, denied
first guest: bytes=65536, ... completion=ok
second guest: bytes=32768, ... completion=ErrorCode::Access
```
