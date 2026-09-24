#![allow(missing_docs)]

use std::path::Path;

#[test]
fn traces_preview_2_and_preview_3_commands() {
    let directory = tempfile::tempdir().unwrap();
    let contents = "middleware kept this output\n".repeat(200);
    std::fs::write(directory.path().join("note.txt"), &contents).unwrap();
    let guests = [
        (
            guest_build::artifact("run-p2-guest"),
            "wasi:io/streams@0.2.12.[method]output-stream.blocking-write-and-flush",
        ),
        (
            guest_build::artifact("run-p3-guest"),
            "wasi:cli/stdout@0.3.0.write-via-stream",
        ),
    ];

    for (guest, expected_call) in guests {
        let output = guest_build::run_package_with_args(
            "run",
            &[guest.to_str().unwrap(), "note.txt"],
            directory.path(),
        )
        .unwrap();

        guest_build::assert_example_succeeded(&output);
        assert_eq!(String::from_utf8(output.stdout).unwrap(), contents);
        let trace = String::from_utf8(output.stderr).unwrap();
        assert!(trace.contains(expected_call), "{trace}");
        assert!(trace.contains("wasi:filesystem/types"), "{trace}");
    }
}

#[test]
fn propagates_guest_exit_codes() {
    for (code, success) in [(0, true), (3, false)] {
        let argument = format!("--exit={code}");
        let guest = guest_build::artifact("run-p2-guest");
        let output = guest_build::run_package_with_args(
            "run",
            &[guest.to_str().unwrap(), &argument],
            Path::new(env!("CARGO_MANIFEST_DIR")),
        )
        .unwrap();

        assert_eq!(output.status.success(), success);
        assert_eq!(output.status.code(), Some(code));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).unwrap();
        assert!(!stderr.contains("wasm backtrace"), "{stderr}");
    }
}
