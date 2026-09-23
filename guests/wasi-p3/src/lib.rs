wit_bindgen::generate!({
    path: "wit",
    world: "workload",
    generate_all,
});

use std::future::{Future, poll_fn};
use std::pin::Pin;
use std::task::Poll;

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

    async fn common_calls() {
        let _ = wasi::cli::environment::get_environment();
        let _ = wasi::cli::environment::get_arguments();
        let _ = wasi::cli::terminal_stdin::get_terminal_stdin();
        let _ = wasi::cli::terminal_stdout::get_terminal_stdout();
        let _ = wasi::cli::terminal_stderr::get_terminal_stderr();
    }

    async fn cancel_wait() {
        let mut wait = Box::pin(wasi::clocks::monotonic_clock::wait_for(u64::MAX));
        poll_fn(|context| {
            assert!(Pin::new(&mut wait).poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(wait);
        let _ = wasi::clocks::monotonic_clock::now();
    }

    async fn abandon_wait() {
        let mut wait = Box::pin(wasi::clocks::monotonic_clock::wait_for(u64::MAX));
        poll_fn(|context| {
            assert!(Pin::new(&mut wait).poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        std::mem::forget(wait);
    }

    async fn exercise() -> String {
        let _ = wasi::cli::environment::get_environment();
        let _ = wasi::cli::environment::get_arguments();
        let _ = wasi::cli::environment::get_initial_cwd();
        let _ = wasi::clocks::monotonic_clock::get_resolution();
        let mut wait_until = Box::pin(wasi::clocks::monotonic_clock::wait_until(5_000_000_000));
        poll_fn(|context| match wait_until.as_mut().poll(context) {
            Poll::Ready(()) => Poll::Ready(()),
            Poll::Pending => panic!("wait-until used duration semantics"),
        })
        .await;
        let mut wait_for = Box::pin(wasi::clocks::monotonic_clock::wait_for(5_000_000_000));
        poll_fn(|context| {
            assert!(wait_for.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(wait_for);
        let _ = wasi::clocks::monotonic_clock::now();
        let _ = wasi::clocks::system_clock::now();
        let _ = wasi::clocks::system_clock::get_resolution();

        let (mut input, completion) = wasi::cli::stdin::read_via_stream();
        let mut stdin = Vec::new();
        loop {
            let (status, bytes) = input.read(Vec::with_capacity(64)).await;
            stdin.extend(bytes);
            if matches!(
                status,
                wit_bindgen::rt::async_support::StreamResult::Dropped
            ) {
                break;
            }
        }
        completion.await.unwrap();

        macro_rules! write_stream {
            ($write:path, $bytes:expr) => {{
                let (mut writer, reader) = wit_stream::new::<u8>();
                let completion = $write(reader);
                assert!(writer.write_all($bytes.to_vec()).await.is_empty());
                drop(writer);
                completion.await.unwrap();
            }};
        }
        write_stream!(wasi::cli::stdout::write_via_stream, b"p3 stdout");
        write_stream!(wasi::cli::stderr::write_via_stream, b"p3 stderr");

        let _ = wasi::cli::terminal_stdin::get_terminal_stdin();
        let _ = wasi::cli::terminal_stdout::get_terminal_stdout();
        let _ = wasi::cli::terminal_stderr::get_terminal_stderr();
        let random_bytes = wasi::random::random::get_random_bytes(4);
        let random_u64 = wasi::random::random::get_random_u64();
        let insecure_bytes = wasi::random::insecure::get_insecure_random_bytes(4);
        let insecure_u64 = wasi::random::insecure::get_insecure_random_u64();
        let insecure_seed = wasi::random::insecure_seed::get_insecure_seed();
        let filesystem = exercise_filesystem().await;

        format!(
            "{}|random={random_bytes:?}:{random_u64}|insecure={insecure_bytes:?}:{insecure_u64}:{insecure_seed:?}|{filesystem}",
            String::from_utf8(stdin).unwrap()
        )
    }

    async fn exit_success() {
        wasi::cli::exit::exit(Ok(()));
    }

    async fn exit_code() {
        wasi::cli::exit::exit_with_code(7);
    }

    async fn refused_open() -> bool {
        use wasi::filesystem::types::{DescriptorFlags, ErrorCode, OpenFlags, PathFlags};

        let directories = wasi::filesystem::preopens::get_directories();
        matches!(
            directories[0]
                .0
                .open_at(
                    PathFlags::empty(),
                    "note.txt".to_owned(),
                    OpenFlags::empty(),
                    DescriptorFlags::READ,
                )
                .await,
            Err(ErrorCode::Access)
        )
    }

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

    async fn stream_write(path: String, size: u64) -> String {
        use wasi::filesystem::types::{DescriptorFlags, OpenFlags, PathFlags};

        let (root, _) = wasi::filesystem::preopens::get_directories().remove(0);
        let file = root
            .open_at(
                PathFlags::empty(),
                path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE,
                DescriptorFlags::WRITE,
            )
            .await
            .unwrap();
        let split = size.saturating_sub(size / 4);
        let initial = generated_bytes(split);
        let appended = generated_bytes(size - split);
        let (mut writer, reader) = wit_stream::new::<u8>();
        let completion = file.write_via_stream(reader, 0);
        assert!(writer.write_all(initial).await.is_empty());
        drop(writer);
        let first = completion.await;
        let (mut writer, reader) = wit_stream::new::<u8>();
        let completion = file.append_via_stream(reader);
        assert!(writer.write_all(appended).await.is_empty());
        drop(writer);
        let second = completion.await;
        format!("write={first:?}, append={second:?}")
    }

    async fn stream_write_tolerant(path: String, size: u64) -> String {
        use wasi::filesystem::types::{DescriptorFlags, OpenFlags, PathFlags};

        let (root, _) = wasi::filesystem::preopens::get_directories().remove(0);
        let file = root
            .open_at(
                PathFlags::empty(),
                path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE,
                DescriptorFlags::WRITE,
            )
            .await
            .unwrap();
        let bytes = generated_bytes(size);
        let offered = bytes.len();
        let (mut writer, reader) = wit_stream::new::<u8>();
        let completion = file.write_via_stream(reader, 0);
        let leftover = writer.write_all(bytes).await.len();
        drop(writer);
        let completion = completion.await;
        format!(
            "offered={offered}, acknowledged={}, leftover={leftover}, completion={completion:?}",
            offered - leftover
        )
    }

    async fn cancel_stream_read(path: String) {
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
        let mut read = Box::pin(stream.read(Vec::with_capacity(64 * 1024)));
        poll_fn(|context| {
            let _ = read.as_mut().poll(context);
            Poll::Ready(())
        })
        .await;
        drop(read);
        drop(stream);
        drop(completion);
    }

    async fn cancel_stream_write(path: String) {
        use wasi::filesystem::types::{DescriptorFlags, OpenFlags, PathFlags};

        let (root, _) = wasi::filesystem::preopens::get_directories().remove(0);
        let file = root
            .open_at(
                PathFlags::empty(),
                path,
                OpenFlags::CREATE | OpenFlags::TRUNCATE,
                DescriptorFlags::WRITE,
            )
            .await
            .unwrap();
        let (mut writer, reader) = wit_stream::new::<u8>();
        let completion = file.write_via_stream(reader, 0);
        let mut write = Box::pin(writer.write_all(generated_bytes(8 * 1024 * 1024)));
        poll_fn(|context| {
            assert!(write.as_mut().poll(context).is_pending());
            Poll::Ready(())
        })
        .await;
        drop(write);
        drop(writer);
        drop(completion);
    }
}

export!(Component);

fn generated_bytes(size: u64) -> Vec<u8> {
    (0..size)
        .map(|index| ((index.wrapping_mul(31) + index / 7) & 0xff) as u8)
        .collect()
}

fn checksum(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |checksum, byte| {
        (checksum ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

async fn exercise_filesystem() -> String {
    use wasi::filesystem::types::{Advice, DescriptorFlags, NewTimestamp, OpenFlags, PathFlags};

    let directories = wasi::filesystem::preopens::get_directories();
    let root = &directories[0].0;
    let file = root
        .open_at(
            PathFlags::empty(),
            "note.txt".to_owned(),
            OpenFlags::empty(),
            DescriptorFlags::READ | DescriptorFlags::WRITE,
        )
        .await
        .unwrap();

    let _ = file.advise(0, 5, Advice::Sequential).await;
    let sync_data = file.sync_data().await.is_ok();
    let flags = file.get_flags().await.unwrap();
    let descriptor_type = file.get_type().await.unwrap();
    let _ = file.set_size(10).await;
    let _ = file
        .set_times(NewTimestamp::NoChange, NewTimestamp::NoChange)
        .await;

    let (mut input, read_completion) = file.read_via_stream(0);
    let (_, streamed) = input.read(Vec::with_capacity(5)).await;
    drop(input);
    read_completion.await.unwrap();

    let (mut writer, reader) = wit_stream::new::<u8>();
    let write_completion = file.write_via_stream(reader, 5);
    assert!(writer.write_all(b"write".to_vec()).await.is_empty());
    drop(writer);
    write_completion.await.unwrap();

    let (mut writer, reader) = wit_stream::new::<u8>();
    let append_completion = file.append_via_stream(reader);
    assert!(writer.write_all(b"append".to_vec()).await.is_empty());
    drop(writer);
    append_completion.await.unwrap();

    let (mut entries, entries_completion) = root.read_directory();
    loop {
        let (status, _) = entries.read(Vec::with_capacity(8)).await;
        if matches!(
            status,
            wit_bindgen::rt::async_support::StreamResult::Dropped
        ) {
            break;
        }
    }
    entries_completion.await.unwrap();
    let sync = root.sync().await.is_ok();
    let created = root.create_directory_at("created".to_owned()).await.is_ok();
    let stat = file.stat().await.unwrap();
    let stat_at = root
        .stat_at(PathFlags::empty(), "note.txt".to_owned())
        .await
        .unwrap();
    let _ = root
        .set_times_at(
            PathFlags::empty(),
            "note.txt".to_owned(),
            NewTimestamp::NoChange,
            NewTimestamp::NoChange,
        )
        .await;
    let linked = root
        .link_at(
            PathFlags::empty(),
            "note.txt".to_owned(),
            root,
            "linked.txt".to_owned(),
        )
        .await
        .is_ok();
    let removed = root
        .remove_directory_at("empty-dir".to_owned())
        .await
        .is_ok();
    let renamed = root
        .rename_at("other.txt".to_owned(), root, "renamed.txt".to_owned())
        .await
        .is_ok();
    let symlinked = root
        .symlink_at("note.txt".to_owned(), "symbolic.txt".to_owned())
        .await
        .is_ok();
    let link_target = root.readlink_at("symbolic.txt".to_owned()).await.unwrap();
    let unlinked = root.unlink_file_at("linked.txt".to_owned()).await.is_ok();
    let same = file.is_same_object(&file).await;
    let hash = file.metadata_hash().await.unwrap();
    let hash_at = root
        .metadata_hash_at(PathFlags::empty(), "note.txt".to_owned())
        .await
        .unwrap();
    let hashes_match = hash.lower == hash_at.lower && hash.upper == hash_at.upper;
    let created_exists = root
        .stat_at(PathFlags::empty(), "created".to_owned())
        .await
        .is_ok();
    let empty_removed = root
        .stat_at(PathFlags::empty(), "empty-dir".to_owned())
        .await
        .is_err();
    let note_exists = root
        .stat_at(PathFlags::empty(), "note.txt".to_owned())
        .await
        .is_ok();
    let other_removed = root
        .stat_at(PathFlags::empty(), "other.txt".to_owned())
        .await
        .is_err();
    let renamed_exists = root
        .stat_at(PathFlags::empty(), "renamed.txt".to_owned())
        .await
        .is_ok();
    let linked_removed = root
        .stat_at(PathFlags::empty(), "linked.txt".to_owned())
        .await
        .is_err();
    let effects = [
        created,
        created_exists,
        removed,
        empty_removed,
        linked,
        note_exists,
        renamed,
        other_removed,
        renamed_exists,
        symlinked,
        link_target == "note.txt",
        unlinked,
        linked_removed,
    ];

    format!(
        "{}:{}:{same}:{}:{flags:?}:{descriptor_type:?}:{sync_data}:{sync}:{effects:?}:{hashes_match}",
        stat.size,
        stat_at.size,
        String::from_utf8_lossy(&streamed)
    )
}
