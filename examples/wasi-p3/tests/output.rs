#![allow(missing_docs)]

#[test]
fn prints_the_trace_and_guest_output() {
    let output = guest_build::run_package("wasi-p3").unwrap();

    guest_build::assert_example_succeeded(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "Hello from WASI at 1700000000.123456789\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        concat!(
            "→ #1 import wasi:cli/environment@0.3.0.get-environment()\n",
            "← #1 returned\n",
            "→ #2 import wasi:clocks/system-clock@0.3.0.now()\n",
            "← #2 returned\n",
            "→ #3 import wasi:cli/stdout@0.3.0.write-via-stream()\n",
            "← #3 returned\n",
        )
    );
}
