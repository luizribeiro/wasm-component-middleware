wit_bindgen::generate!({
    path: "wit",
    world: "workload",
    generate_all,
});

struct Component;

impl Guest for Component {
    async fn stream_read(path: String, slow: bool) -> String {
        use wasi::filesystem::types::{DescriptorFlags, OpenFlags, PathFlags};

        let (root, _) = wasi::filesystem::preopens::get_directories().remove(0);
        let file = root
            .open_at(
                PathFlags::empty(),
                path,
                OpenFlags::empty(),
                DescriptorFlags::READ,
            )
            .await
            .unwrap();
        let (mut stream, completion) = file.read_via_stream(0);
        let mut bytes = Vec::new();
        loop {
            let capacity = if slow { 1024 } else { 64 * 1024 };
            let (status, chunk) = stream.read(Vec::with_capacity(capacity)).await;
            bytes.extend(chunk);
            if slow {
                wasi::clocks::monotonic_clock::wait_for(250_000).await;
            }
            if matches!(
                status,
                wit_bindgen::rt::async_support::StreamResult::Dropped
            ) {
                break;
            }
        }
        let completion = completion
            .await
            .map(|()| "ok".to_owned())
            .unwrap_or_else(|error| format!("{error:?}"));
        format!(
            "bytes={}, checksum={:#018x}, completion={completion}",
            bytes.len(),
            checksum(&bytes)
        )
    }
}

export!(Component);

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |checksum, byte| {
        (checksum ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
