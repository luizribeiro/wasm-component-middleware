#![allow(missing_docs)]

#[test]
fn prints_the_trace_and_refusal() {
    let output = guest_build::run_package("random").unwrap();

    guest_build::assert_example_succeeded(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "dice: 6, 6; bytes: [1, 1, 1, 1]\n"
    );
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        concat!(
            "→ #1 import wasi:random/random@0.2.12.get-random-u64()\n",
            "← #1 returned\n",
            "→ #2 import wasi:random/random@0.2.12.get-random-u64()\n",
            "← #2 returned\n",
            "→ #3 import wasi:random/random@0.2.12.get-random-bytes(len=4)\n",
            "← #3 returned\n",
            "→ #4 import wasi:cli/stdout@0.2.12.get-stdout()\n",
            "← #4 returned produced=[0]\n",
            "→ #5 import wasi:io/streams@0.2.12.[method]output-stream.blocking-write-and-flush(bytes=32 bytes \"dice: 6, 6; bytes: [1, 1…\") handles=[0]\n",
            "← #5 returned\n",
            "→ #6 import wasi:io/streams@0.2.12.[resource-drop]output-stream() handles=[0]\n",
            "← #6 returned\n",
            "→ #7 import wasi:random/random@0.2.12.get-random-u64()\n",
            "← #7 failed: random call limit of 3 reached\n",
            "guest trapped: random call limit of 3 reached\n",
        )
    );
}
