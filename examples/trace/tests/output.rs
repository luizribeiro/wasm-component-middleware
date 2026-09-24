#![allow(missing_docs)]

#[test]
fn prints_nested_calls_and_greeting() {
    let output = guest_build::run_package("trace").unwrap();

    guest_build::assert_example_succeeded(&output);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "Hello, Ada!\n");
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        "→ #1 export greet(greeting=\"Hello\")\n  → #2 import example:hello/host.user-name()\n  ← #2 returned\n  → #3 import example:hello/host.log(message=\"greeting Ada\")\n  ← #3 returned\n← #1 returned\n"
    );
}
