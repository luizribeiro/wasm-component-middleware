#![allow(missing_docs)]

fn strip_ports(output: &str) -> String {
    let mut normalized = String::new();
    let mut rest = output;
    while let Some(index) = rest.find("127.0.0.1:") {
        let (before, after) = rest.split_at(index);
        normalized.push_str(before);
        normalized.push_str("127.0.0.1:<port>");
        rest = after
            .trim_start_matches("127.0.0.1:")
            .trim_start_matches(char::is_numeric);
    }
    normalized.push_str(rest);
    normalized
}

#[test]
fn http_allowlist_example_prints_trace_body_and_denial() {
    let output = guest_build::run_package("http-allowlist").unwrap();

    guest_build::assert_example_succeeded(&output);
    assert_eq!(
        strip_ports(&String::from_utf8(output.stdout).unwrap()),
        concat!(
            "allowed body: GET http://127.0.0.1:<port>/message x-client=middleware -> 201 x-server=loopback | hello from the allowed server\n",
            "denied error: ErrorCode::HttpRequestDenied\n",
        )
    );
    assert_eq!(
        strip_ports(&String::from_utf8(output.stderr).unwrap()),
        concat!(
            "→ #1 import wasi:http/request-hook.[send-request](method=\"GET\", scheme=\"http\", authority=\"127.0.0.1:<port>\", path=\"/message\", headers=[\"host: 127.0.0.1:<port>\", \"x-client: middleware\"])\n",
            "← #1 returned\n",
            "→ #2 import wasi:http/request-hook.[send-request](method=\"GET\", scheme=\"http\", authority=\"127.0.0.1:<port>\", path=\"/message\", headers=[\"host: 127.0.0.1:<port>\", \"x-client: middleware\"])\n",
            "← #2 failed: authority is not allowed\n",
        )
    );
}
