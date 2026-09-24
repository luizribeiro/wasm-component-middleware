#![allow(missing_docs)]

fn strip_loopback_ports(output: &str) -> String {
    const PREFIX: &str = "127.0.0.1:";
    let mut stripped = String::with_capacity(output.len());
    let mut remainder = output;
    while let Some(index) = remainder.find(PREFIX) {
        let port = &remainder[index + PREFIX.len()..];
        let digits = port.chars().take_while(char::is_ascii_digit).count();
        stripped.push_str(&remainder[..index]);
        stripped.push_str(PREFIX);
        stripped.push_str("<port>");
        remainder = &port[digits..];
    }
    stripped.push_str(remainder);
    stripped
}

#[test]
fn traces_addresses_and_reports_access() {
    let output = guest_build::run_package("net-allowlist").unwrap();

    guest_build::assert_example_succeeded(&output);
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        concat!(
            "p2:\n",
            "allowed: hello\n",
            "denied: access-denied\n",
            "udp allowed: delivered\n",
            "udp denied: access-denied\n",
            "p3:\n",
            "allowed: hello\n",
            "denied: access-denied\n",
            "udp allowed: delivered\n",
            "udp denied: access-denied\n",
            "udp connected denied: access-denied\n",
        )
    );
    let trace = strip_loopback_ports(&String::from_utf8(output.stderr).unwrap());
    let connects = trace
        .lines()
        .filter(|line| line.contains("[method]tcp-socket.start-connect"))
        .collect::<Vec<_>>();
    assert_eq!(connects.len(), 2, "{trace}");
    assert!(connects.iter().all(|line| {
        line.contains("remote_address=\"127.0.0.1:<port>\"") && line.contains("handles=")
    }));
    let streams = trace
        .lines()
        .filter(|line| line.contains("[method]udp-socket.stream"))
        .collect::<Vec<_>>();
    assert_eq!(streams.len(), 2, "{trace}");
    assert!(
        streams
            .iter()
            .all(|line| line.contains("remote_address=none") && line.contains("handles="))
    );
    let sends = trace
        .lines()
        .filter(|line| line.contains("[method]outgoing-datagram-stream.send"))
        .collect::<Vec<_>>();
    assert_eq!(sends.len(), 2, "{trace}");
    assert!(
        sends.iter().all(|line| {
            line.contains("some(\"127.0.0.1:<port>\")") && line.contains("handles=")
        })
    );
    let p3_connects = trace
        .lines()
        .filter(|line| line.contains("[method]tcp-socket.connect"))
        .collect::<Vec<_>>();
    assert_eq!(p3_connects.len(), 2, "{trace}");
    assert!(p3_connects.iter().all(|line| {
        line.contains("remote_address=\"127.0.0.1:<port>\"") && line.contains("handles=")
    }));
    let p3_udp_connects = trace
        .lines()
        .filter(|line| line.contains("[method]udp-socket.connect"))
        .collect::<Vec<_>>();
    assert_eq!(p3_udp_connects.len(), 1, "{trace}");
    assert!(p3_udp_connects.iter().all(|line| {
        line.contains("remote_address=\"127.0.0.1:<port>\"") && line.contains("handles=")
    }));
    let p3_sends = trace
        .lines()
        .filter(|line| line.contains("[method]udp-socket.send"))
        .collect::<Vec<_>>();
    assert_eq!(p3_sends.len(), 2, "{trace}");
    assert!(p3_sends.iter().all(|line| {
        line.contains("data=3 bytes \"udp\"")
            && line.contains("remote_address=some(\"127.0.0.1:<port>\")")
            && line.contains("handles=")
    }));
    assert!(trace.contains("returned\n"), "{trace}");
    assert_eq!(
        trace
            .matches("failed: remote address is not allowed\n")
            .count(),
        5,
        "{trace}"
    );
}
