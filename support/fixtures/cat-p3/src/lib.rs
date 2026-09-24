wit_bindgen::generate!({
    path: "wit",
    world: "cat-p3",
    generate_all,
});

struct Component;

impl exports::wasi::cli::run::Guest for Component {
    async fn run() -> Result<(), ()> {
        let path = wasi::cli::environment::get_arguments()
            .into_iter()
            .nth(1)
            .ok_or(())?;
        let (root, _) = wasi::filesystem::preopens::get_directories()
            .into_iter()
            .next()
            .ok_or(())?;
        let file = root
            .open_at(
                wasi::filesystem::types::PathFlags::empty(),
                path,
                wasi::filesystem::types::OpenFlags::empty(),
                wasi::filesystem::types::DescriptorFlags::READ,
            )
            .await
            .map_err(|_| ())?;
        let (mut input, completion) = file.read_via_stream(0);
        let mut bytes = Vec::new();
        loop {
            let (status, chunk) = input.read(Vec::with_capacity(4096)).await;
            bytes.extend(chunk);
            if matches!(
                status,
                wit_bindgen::rt::async_support::StreamResult::Dropped
            ) {
                break;
            }
        }
        completion.await.map_err(|_| ())?;

        let (mut output, stream) = wit_stream::new::<u8>();
        let completion = wasi::cli::stdout::write_via_stream(stream);
        if !output.write_all(bytes).await.is_empty() {
            return Err(());
        }
        drop(output);
        completion.await.map_err(|_| ())
    }
}

export!(Component);
