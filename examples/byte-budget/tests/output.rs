#![allow(missing_docs)]

#[test]
fn reports_the_relay_denial() {
    let output = guest_build::run_package("byte-budget").unwrap();

    guest_build::assert_example_succeeded(&output);
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "stream read: 106496 bytes in 13 chunks, denied\n"
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "first guest: bytes=65536, checksum=0xe5f393b4c91013d4, completion=ok\n",
            "second guest: bytes=32768, checksum=0x9c33ddd05aba538c, ",
            "completion=ErrorCode::Access\n",
        )
    );
}
