# Sandbox

This example combines descriptor-aware path policy with a four-descriptor open
file limit. It allows a public preopen and refuses a private one.

Run it from the repository root:

```console
cargo run -p sandbox
```

The guest reports the visible policy outcomes:

```text
read public/note.txt: hello
read private/secret.txt: access
escape from public preopen: access
...
open public/three.txt: access
```
