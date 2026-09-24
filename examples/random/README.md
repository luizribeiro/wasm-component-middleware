# Random

This example replaces random values with deterministic bytes and refuses the
fourth random call with a middleware budget.

Run it from the repository root:

```console
cargo run -p random
```

The guest prints deterministic values before the final call is refused:

```text
dice: 6, 6; bytes: [1, 1, 1, 1]
...
guest trapped: random call limit of 3 reached
```
