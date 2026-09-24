#![allow(missing_docs)]

#[test]
fn refuses_one_store_and_allows_the_next() {
    let output = guest_build::run_package("deny").unwrap();

    guest_build::assert_example_succeeded(&output);
    assert!(output.stdout.is_empty());
    assert_eq!(
        String::from_utf8(output.stderr).unwrap(),
        concat!(
            "denied:\n",
            "→ #1 export greet(greeting=\"Hello\")\n",
            "  → #2 import example:hello/host.user-name()\n",
            "  ← #2 failed: import example:hello/host.user-name is not allowed\n",
            "← #1 failed: import example:hello/host.user-name is not allowed\n",
            "greet failed: import example:hello/host.user-name is not allowed\n",
            "allowed:\n",
            "→ #1 export greet(greeting=\"Hello\")\n",
            "  → #2 import example:hello/host.user-name()\n",
            "  ← #2 returned\n",
            "  → #3 import example:hello/host.log(message=\"greeting Ada\")\n",
            "  ← #3 returned\n",
            "← #1 returned\n",
            "Hello, Ada!\n",
        )
    );
}
