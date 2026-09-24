//! Builds isolated WebAssembly guest workspaces for host-side use.

use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = PathBuf::from(required_var("CARGO_MANIFEST_DIR")?);
    let repository = crate_dir.join("../..");
    let fixture_manifest = repository.join("support/fixtures/Cargo.toml");
    let out_dir = PathBuf::from(required_var("OUT_DIR")?);
    let main_target_dir = out_dir
        .ancestors()
        .nth(4)
        .ok_or_else(|| io::Error::other("OUT_DIR is not inside Cargo's target directory"))?;
    let guest_target_dir = main_target_dir.join("guest-build");

    build_guest_workspace(&fixture_manifest, &guest_target_dir)?;
    build_example_guests(&repository, &guest_target_dir)?;

    let release_dir = guest_target_dir.join("wasm32-wasip2/release");
    emit_guest_path("CAT_P2_COMPONENT", &release_dir.join("cat_p2.wasm"));
    emit_guest_path("CAT_P3_COMPONENT", &release_dir.join("cat_p3.wasm"));
    emit_guest_path("HELLO_COMPONENT", &release_dir.join("hello.wasm"));
    emit_guest_path("HTTP_P2_COMPONENT", &release_dir.join("http_p2.wasm"));
    emit_guest_path("HTTP_P3_COMPONENT", &release_dir.join("http_p3.wasm"));
    emit_guest_path(
        "NETWORK_ERROR_CODE_COMPONENT",
        &release_dir.join("network_error_code.wasm"),
    );
    emit_guest_path("SANDBOX_COMPONENT", &release_dir.join("sandbox.wasm"));
    emit_guest_path("SMOKE_P2_COMPONENT", &release_dir.join("smoke_p2.wasm"));
    emit_guest_path("SMOKE_P3_COMPONENT", &release_dir.join("smoke_p3.wasm"));
    emit_guest_path(
        "UNROUTED_IMPORT_COMPONENT",
        &release_dir.join("unrouted_import.wasm"),
    );
    emit_guest_path("WASI_P2_COMPONENT", &release_dir.join("wasi_p2.wasm"));
    emit_guest_path("WASI_P3_COMPONENT", &release_dir.join("wasi_p3.wasm"));
    println!("cargo::rustc-env=GUEST_BUILD_DIR={}", release_dir.display());
    println!(
        "cargo::rerun-if-changed={}",
        repository.join("support/fixtures").display()
    );
    let examples = repository.join("examples");
    if examples.exists() {
        println!("cargo::rerun-if-changed={}", examples.display());
    }
    Ok(())
}

fn build_example_guests(repository: &Path, target_dir: &Path) -> io::Result<()> {
    let examples = repository.join("examples");
    if !examples.exists() {
        return Ok(());
    }
    for entry in std::fs::read_dir(examples)? {
        let manifest = entry?.path().join("guest/Cargo.toml");
        if manifest.is_file() {
            build_guest_workspace(&manifest, target_dir)?;
        }
    }
    Ok(())
}

fn build_guest_workspace(manifest: &Path, target_dir: &Path) -> io::Result<()> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.args([
        OsStr::new("build"),
        OsStr::new("--release"),
        OsStr::new("--target"),
        OsStr::new("wasm32-wasip2"),
        OsStr::new("--manifest-path"),
        manifest.as_os_str(),
        OsStr::new("--target-dir"),
        target_dir.as_os_str(),
        OsStr::new("--locked"),
    ]);
    for (key, _) in env::vars_os() {
        if key.to_string_lossy().starts_with("CARGO_") || key == "RUSTFLAGS" {
            command.env_remove(key);
        }
    }

    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "guest build failed with {status}"
        )));
    }
    Ok(())
}

fn required_var(name: &str) -> io::Result<std::ffi::OsString> {
    env::var_os(name).ok_or_else(|| io::Error::other(format!("{name} is not set")))
}

fn emit_guest_path(name: &str, path: &Path) {
    println!("cargo::rustc-env={name}={}", path.display());
}
