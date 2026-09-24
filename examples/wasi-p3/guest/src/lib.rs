wit_bindgen::generate!({
    path: "wit",
    world: "workload",
    generate_all,
});

struct Component;

impl Guest for Component {
    async fn basic_system() {
        let greeting = wasi::cli::environment::get_environment()
            .into_iter()
            .find_map(|(name, value)| (name == "GREETING").then_some(value))
            .unwrap_or_default();
        let now = wasi::clocks::system_clock::now();
        let line = format!("{greeting} at {}.{:09}\n", now.seconds, now.nanoseconds);
        let (mut writer, reader) = wit_stream::new::<u8>();
        let completion = wasi::cli::stdout::write_via_stream(reader);
        assert!(writer.write_all(line.into_bytes()).await.is_empty());
        drop(writer);
        completion.await.unwrap();
    }
}

export!(Component);
