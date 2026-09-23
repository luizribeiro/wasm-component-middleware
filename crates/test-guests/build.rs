//! Builds the isolated WebAssembly guest workspace for host-side tests.

use std::env;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = PathBuf::from(required_var("CARGO_MANIFEST_DIR")?);
    let guest_manifest = crate_dir.join("../../guests/Cargo.toml");
    let out_dir = PathBuf::from(required_var("OUT_DIR")?);
    let main_target_dir = out_dir
        .ancestors()
        .nth(4)
        .ok_or_else(|| io::Error::other("OUT_DIR is not inside Cargo's target directory"))?;
    let guest_target_dir = main_target_dir.join("guest-build");

    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = Command::new(cargo);
    command.args([
        OsStr::new("build"),
        OsStr::new("--release"),
        OsStr::new("--target"),
        OsStr::new("wasm32-wasip2"),
        OsStr::new("--manifest-path"),
        guest_manifest.as_os_str(),
        OsStr::new("--target-dir"),
        guest_target_dir.as_os_str(),
        OsStr::new("--locked"),
    ]);
    for (key, _) in env::vars_os() {
        if key.to_string_lossy().starts_with("CARGO_") || key == "RUSTFLAGS" {
            command.env_remove(key);
        }
    }

    let status = command.status()?;
    if !status.success() {
        return Err(io::Error::other(format!("guest build failed with {status}")).into());
    }

    let release_dir = guest_target_dir.join("wasm32-wasip2/release");
    emit_guest_path("HELLO_COMPONENT", &release_dir.join("hello.wasm"));
    emit_guest_path("SMOKE_P2_COMPONENT", &release_dir.join("smoke_p2.wasm"));
    emit_guest_path("SMOKE_P3_COMPONENT", &release_dir.join("smoke_p3.wasm"));
    emit_guest_path(
        "UNROUTED_IMPORT_COMPONENT",
        &release_dir.join("unrouted_import.wasm"),
    );
    emit_guest_path("WASI_P2_COMPONENT", &release_dir.join("wasi_p2.wasm"));
    println!(
        "cargo::rerun-if-changed={}",
        crate_dir.join("../../guests").display()
    );
    Ok(())
}

fn required_var(name: &str) -> io::Result<std::ffi::OsString> {
    env::var_os(name).ok_or_else(|| io::Error::other(format!("{name} is not set")))
}

fn emit_guest_path(name: &str, path: &Path) {
    println!("cargo::rustc-env={name}={}", path.display());
}
