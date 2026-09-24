wit_bindgen::generate!({
    path: "wit",
    world: "workload",
});

struct Component;

impl Guest for Component {
    fn random_demo() {
        let first = wasi::random::random::get_random_u64() % 6 + 1;
        let second = wasi::random::random::get_random_u64() % 6 + 1;
        let bytes = wasi::random::random::get_random_bytes(4);
        let line = format!("dice: {first}, {second}; bytes: {bytes:?}\n");
        wasi::cli::stdout::get_stdout()
            .blocking_write_and_flush(line.as_bytes())
            .unwrap();
        let _ = wasi::random::random::get_random_u64();
    }
}

export!(Component);
