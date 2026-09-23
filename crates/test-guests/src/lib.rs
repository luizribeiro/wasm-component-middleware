//! Build-time access to WebAssembly components used by workspace tests.
//!
//! The components are built in an isolated guest workspace so host workspace
//! commands never compile guest crates for the native target.

use std::fs::{File, OpenOptions};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const BUILD_TIMEOUT: Duration = Duration::from_secs(300);
const EXAMPLE_TIMEOUT: Duration = Duration::from_secs(60);

/// Runs a workspace example for an output assertion with a bounded lifetime.
///
/// Use this in integration tests that verify an example's standard output and
/// standard error. A timed-out subprocess is killed and reported as an I/O
/// error so one stuck executable cannot block the test suite indefinitely.
///
/// # Errors
///
/// Returns an error when the process cannot be started, its output cannot be
/// collected, its build exceeds five minutes, or two launch attempts each
/// exceed one minute.
pub fn run_example(package: &str, example: &str) -> io::Result<Output> {
    let lock = example_lock()?;
    let workspace = workspace_root();
    let mut build = Command::new(env!("CARGO"));
    build.current_dir(&workspace).args([
        "build",
        "--quiet",
        "-p",
        package,
        "--example",
        example,
        "--locked",
    ]);
    let build = output_with_timeout(&mut build, BUILD_TIMEOUT)?;
    if !build.status.success() {
        return Err(io::Error::other(format!(
            "example build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        )));
    }
    drop(lock);

    let executable = example_executable(example);
    for attempt in 0..2 {
        let mut command = Command::new(&executable);
        command.current_dir(&workspace);
        match output_with_timeout(&mut command, EXAMPLE_TIMEOUT) {
            Err(error) if error.kind() == io::ErrorKind::TimedOut && attempt == 0 => {}
            result => return result,
        }
    }
    unreachable!()
}

fn example_lock() -> io::Result<File> {
    let target = target_dir();
    std::fs::create_dir_all(&target)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(target.join("example-tests.lock"))?;
    lock.lock()?;
    Ok(lock)
}

fn example_executable(example: &str) -> PathBuf {
    let mut path = target_dir().join("debug/examples").join(example);
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
}

fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR")
        .map_or_else(|| workspace_root().join("target"), PathBuf::from)
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn output_with_timeout(command: &mut Command, timeout: Duration) -> io::Result<Output> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("example stdout was not captured"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| io::Error::other("example stderr was not captured"))?;
    let stdout = thread::spawn(move || read_all(stdout));
    let stderr = thread::spawn(move || read_all(stderr));
    let deadline = Instant::now() + timeout;

    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            join_reader(stdout)?;
            join_reader(stderr)?;
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                format!("example exceeded {} seconds", timeout.as_secs()),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };

    Ok(Output {
        status,
        stdout: join_reader(stdout)?,
        stderr: join_reader(stderr)?,
    })
}

fn read_all(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn join_reader(reader: thread::JoinHandle<io::Result<Vec<u8>>>) -> io::Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| io::Error::other("example output reader panicked"))?
}

/// Returns the path to the guest that imports host identity and logging.
#[must_use]
pub fn hello() -> &'static Path {
    Path::new(env!("HELLO_COMPONENT"))
}

/// Returns the path to the Preview 2 outgoing HTTP client.
#[must_use]
pub fn http_p2() -> &'static Path {
    Path::new(env!("HTTP_P2_COMPONENT"))
}

/// Returns the path to the Preview 3 outgoing HTTP client.
#[must_use]
pub fn http_p3() -> &'static Path {
    Path::new(env!("HTTP_P3_COMPONENT"))
}

/// Returns the path to the guest that imports unstable socket error conversion.
#[must_use]
pub fn network_error_code() -> &'static Path {
    Path::new(env!("NETWORK_ERROR_CODE_COMPONENT"))
}

/// Returns the path to the guest that performs sandboxed filesystem work.
#[must_use]
pub fn sandbox() -> &'static Path {
    Path::new(env!("SANDBOX_COMPONENT"))
}

/// Returns the path to the synchronous Preview 2 smoke component.
#[must_use]
pub fn smoke_p2() -> &'static Path {
    Path::new(env!("SMOKE_P2_COMPONENT"))
}

/// Returns the path to the asynchronous Preview 3 smoke component.
#[must_use]
pub fn smoke_p3() -> &'static Path {
    Path::new(env!("SMOKE_P3_COMPONENT"))
}

/// Returns the path to the guest that imports a host interface left unrouted.
#[must_use]
pub fn unrouted_import() -> &'static Path {
    Path::new(env!("UNROUTED_IMPORT_COMPONENT"))
}

/// Returns the path to the guest that exercises synchronous Preview 2 calls.
#[must_use]
pub fn wasi_p2() -> &'static Path {
    Path::new(env!("WASI_P2_COMPONENT"))
}

/// Returns the path to the guest that exercises concurrent Preview 3 calls.
#[must_use]
pub fn wasi_p3() -> &'static Path {
    Path::new(env!("WASI_P3_COMPONENT"))
}

#[cfg(test)]
mod tests {
    use wasmtime::component::{Component, Linker, ResourceTable};
    use wasmtime::{Config, Engine, Result, Store};
    use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

    mod p2 {
        wasmtime::component::bindgen!({
            path: "../../guests/smoke-p2/wit",
            world: "smoke-p2",
            exports: { default: async },
            require_store_data_send: true,
        });
    }

    mod p3 {
        wasmtime::component::bindgen!({
            path: "../../guests/smoke-p3/wit",
            world: "smoke-p3",
            exports: { default: async | store },
            require_store_data_send: true,
        });
    }

    struct State {
        table: ResourceTable,
        wasi: WasiCtx,
    }

    impl State {
        fn new() -> Self {
            Self {
                table: ResourceTable::new(),
                wasi: WasiCtxBuilder::new().build(),
            }
        }
    }

    impl WasiView for State {
        fn ctx(&mut self) -> WasiCtxView<'_> {
            WasiCtxView {
                ctx: &mut self.wasi,
                table: &mut self.table,
            }
        }
    }

    fn engine(concurrent: bool) -> Result<Engine> {
        let mut config = Config::new();
        if concurrent {
            config.wasm_component_model_async(true);
            config.concurrency_support(true);
        }
        Engine::new(&config)
    }

    fn linker(engine: &Engine) -> Result<Linker<State>> {
        let mut linker = Linker::new(engine);
        wasmtime_wasi::p2::add_to_linker_async(&mut linker)?;
        Ok(linker)
    }

    #[test]
    fn loads_unrouted_import_guest() -> Result<()> {
        let engine = engine(false)?;
        Component::from_file(&engine, super::unrouted_import())?;
        Ok(())
    }

    #[tokio::test]
    async fn instantiates_p2_guest_and_greets() -> Result<()> {
        let engine = engine(false)?;
        let linker = linker(&engine)?;
        let component = Component::from_file(&engine, super::smoke_p2())?;
        let mut store = Store::new(&engine, State::new());
        let guest = p2::SmokeP2::instantiate_async(&mut store, &component, &linker).await?;

        let greeting = guest
            .test_smoke_p2_greeter()
            .call_greet(&mut store, "Ada")
            .await?;

        assert_eq!(greeting, "Hello, Ada!");
        Ok(())
    }

    #[tokio::test]
    async fn instantiates_p3_guest_and_greets_concurrently() -> Result<()> {
        let engine = engine(true)?;
        let linker = linker(&engine)?;
        let component = Component::from_file(&engine, super::smoke_p3())?;
        let mut store = Store::new(&engine, State::new());
        let guest = p3::SmokeP3::instantiate_async(&mut store, &component, &linker).await?;

        let greeting = store
            .run_concurrent(async move |accessor| {
                guest
                    .test_smoke_p3_greeter()
                    .call_greet(accessor, "Grace".to_owned())
                    .await
            })
            .await??;

        assert_eq!(greeting, "Hello, Grace!");
        Ok(())
    }
}
