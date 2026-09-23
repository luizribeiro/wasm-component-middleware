wit_bindgen::generate!({
    path: "wit",
    world: "workload",
});

use std::io::Read;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Component;

impl Guest for Component {
    fn observe() -> String {
        let greeting = std::env::var("GREETING").unwrap_or_default();
        let arguments = std::env::args().collect::<Vec<_>>().join("|");
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let monotonic = Instant::now();
        std::thread::sleep(Duration::ZERO);
        let elapsed = monotonic.elapsed();
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        println!("stdout:{greeting}");
        eprintln!("stderr:{arguments}");
        format!(
            "{greeting};{arguments};{}:{};{};{input}",
            wall.as_secs(),
            wall.subsec_nanos(),
            elapsed.as_nanos()
        )
    }

    fn basic_system() {
        let greeting = std::env::var("GREETING").unwrap_or_default();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let line = format!(
            "{greeting} at {}.{:09}\n",
            now.as_secs(),
            now.subsec_nanos()
        );
        wasi::cli::stdout::get_stdout()
            .blocking_write_and_flush(line.as_bytes())
            .unwrap();
    }

    fn write_stdout() -> bool {
        let stdout = wasi::cli::stdout::get_stdout();
        match stdout.blocking_write_and_flush(b"guest write\n") {
            Err(wasi::io::streams::StreamError::LastOperationFailed(error)) => {
                let _ = error.to_debug_string();
                true
            }
            Err(wasi::io::streams::StreamError::Closed) | Ok(()) => false,
        }
    }

    fn exercise() -> String {
        let environment = wasi::cli::environment::get_environment();
        let arguments = wasi::cli::environment::get_arguments();
        let initial_cwd = wasi::cli::environment::initial_cwd();
        let wall = wasi::clocks::wall_clock::now();
        let wall_resolution = wasi::clocks::wall_clock::resolution();
        let instant = wasi::clocks::monotonic_clock::now();
        let instant_resolution = wasi::clocks::monotonic_clock::resolution();
        let instant_pollable = wasi::clocks::monotonic_clock::subscribe_instant(instant);
        let duration_pollable = wasi::clocks::monotonic_clock::subscribe_duration(0);
        let instant_ready = instant_pollable.ready();
        duration_pollable.block();
        let clock_poll = wasi::io::poll::poll(&[&instant_pollable, &duration_pollable]);

        let input = wasi::cli::stdin::get_stdin();
        let read = input.read(1).unwrap();
        let blocking_read = input.blocking_read(1).unwrap();
        let skipped = input.skip(1).unwrap();
        let blocking_skipped = input.blocking_skip(1).unwrap();
        let input_pollable = input.subscribe();
        input_pollable.block();
        let input_ready = input_pollable.ready();

        let output = wasi::cli::stdout::get_stdout();
        let write_permit = output.check_write().unwrap();
        output.write(b"w").unwrap();
        output.blocking_write_and_flush(b"b").unwrap();
        output.blocking_write_zeroes_and_flush(1).unwrap();
        let output_pollable = output.subscribe();
        output_pollable.block();
        let output_ready = output_pollable.ready();
        output.write_zeroes(1).unwrap();
        output.flush().unwrap();
        output.blocking_flush().unwrap();
        let spliced = output.splice(&input, 1).unwrap();
        let blocking_spliced = output.blocking_splice(&input, 1).unwrap();

        let _ = wasi::cli::stderr::get_stderr();
        let terminal_stdin = wasi::cli::terminal_stdin::get_terminal_stdin().is_some();
        let terminal_stdout = wasi::cli::terminal_stdout::get_terminal_stdout().is_some();
        let terminal_stderr = wasi::cli::terminal_stderr::get_terminal_stderr().is_some();
        drop(input_pollable);
        drop(output_pollable);

        format!(
            "{environment:?}|{arguments:?}|{initial_cwd:?}|{}:{}|{}:{}|{instant}|{instant_resolution}|{instant_ready}|{clock_poll:?}|{read:?}|{blocking_read:?}|{skipped}|{blocking_skipped}|{input_ready}|{write_permit}|{output_ready}|{spliced}|{blocking_spliced}|{terminal_stdin}|{terminal_stdout}|{terminal_stderr}",
            wall.seconds, wall.nanoseconds, wall_resolution.seconds, wall_resolution.nanoseconds,
        )
    }

    fn exit_success() {
        wasi::cli::exit::exit(Ok(()));
    }

    fn exit_code() {
        wasi::cli::exit::exit_with_code(7);
    }

    fn common_calls() {
        let _ = wasi::cli::environment::get_environment();
        let _ = wasi::cli::environment::get_arguments();
        let _ = wasi::cli::terminal_stdin::get_terminal_stdin();
        let _ = wasi::cli::terminal_stdout::get_terminal_stdout();
        let _ = wasi::cli::terminal_stderr::get_terminal_stderr();
    }
}

export!(Component);
