#![allow(missing_docs)]

#[test]
fn prints_the_trace_and_guest_output() {
    let output = guest_build::run_package("wasi-p2").unwrap();

    guest_build::assert_example_succeeded(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Hello from WASI at 1700000000.123456789\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        concat!(
            "→ #1 import wasi:cli/environment@0.2.12.get-environment()\n",
            "← #1 returned\n",
            "→ #2 import wasi:clocks/wall-clock@0.2.12.now()\n",
            "← #2 returned\n",
            "→ #3 import wasi:cli/stdout@0.2.12.get-stdout()\n",
            "← #3 returned produced=[0]\n",
            "→ #4 import wasi:io/streams@0.2.12.[method]output-stream.blocking-write-and-flush(bytes=40 bytes \"Hello from WASI at 17000…\") handles=[0]\n",
            "← #4 returned\n",
            "→ #5 import wasi:io/streams@0.2.12.[resource-drop]output-stream() handles=[0]\n",
            "← #5 returned\n",
        )
    );
}
