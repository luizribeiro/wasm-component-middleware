#![allow(missing_docs)]

#[test]
fn prints_file_policy_results_and_handle_traces() {
    let output = guest_build::run_package("sandbox").unwrap();

    guest_build::assert_example_succeeded(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "read public/note.txt: hello\n",
            "read private/secret.txt: access\n",
            "escape from public preopen: access\n",
            "open public/one.txt: allowed\n",
            "open public/two.txt: allowed\n",
            "open public/three.txt: access\n",
        )
    );
    let trace = String::from_utf8(output.stderr).unwrap();
    assert!(trace.contains("get-directories()\n← #1 returned produced=[0, 1]"));
    assert!(trace.contains("[resource-drop]descriptor() handles=["));
    assert!(trace.contains("failed: open file limit of 4 reached"));
    assert!(trace.contains("failed: descriptor is private"));
    assert!(trace.contains("failed: not-permitted"));
}
