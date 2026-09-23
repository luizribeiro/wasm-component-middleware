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

        String::from_utf8(stdin).unwrap()
    }

    async fn exit_success() {
        wasi::cli::exit::exit(Ok(()));
    }

    async fn exit_code() {
        wasi::cli::exit::exit_with_code(7);
    }
}

export!(Component);
