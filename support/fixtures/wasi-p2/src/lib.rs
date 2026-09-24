wit_bindgen::generate!({
    path: "wit",
    world: "workload",
});

use std::io::Read;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Component;

impl Guest for Component {
    fn observe() -> String {
        let greeting = std::env::var("GREETING").unwrap_or_default();
        let arguments = std::env::args().collect::<Vec<_>>().join("|");
        let wall = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let monotonic = Instant::now();
        std::thread::sleep(Duration::ZERO);
        let elapsed = monotonic.elapsed();
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input).unwrap();
        println!("stdout:{greeting}");
        eprintln!("stderr:{arguments}");
        format!(
            "{greeting};{arguments};{}:{};{};{input}",
            wall.as_secs(),
            wall.subsec_nanos(),
            elapsed.as_nanos()
        )
    }

    fn basic_system() {
        let greeting = std::env::var("GREETING").unwrap_or_default();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let line = format!(
            "{greeting} at {}.{:09}\n",
            now.as_secs(),
            now.subsec_nanos()
        );
        wasi::cli::stdout::get_stdout()
            .blocking_write_and_flush(line.as_bytes())
            .unwrap();
    }

    fn write_stdout() -> bool {
        let stdout = wasi::cli::stdout::get_stdout();
        match stdout.blocking_write_and_flush(b"guest write\n") {
            Err(wasi::io::streams::StreamError::LastOperationFailed(error)) => {
                let _ = wasi::filesystem::types::filesystem_error_code(&error);
                let _ = error.to_debug_string();
                true
            }
            Err(wasi::io::streams::StreamError::Closed) | Ok(()) => false,
        }
    }

    fn exercise() -> String {
        let environment = wasi::cli::environment::get_environment();
        let arguments = wasi::cli::environment::get_arguments();
        let initial_cwd = wasi::cli::environment::initial_cwd();
        let wall = wasi::clocks::wall_clock::now();
        let wall_resolution = wasi::clocks::wall_clock::resolution();
        let instant = wasi::clocks::monotonic_clock::now();
        let instant_resolution = wasi::clocks::monotonic_clock::resolution();
        let instant_pollable = wasi::clocks::monotonic_clock::subscribe_instant(instant);
        let duration_pollable = wasi::clocks::monotonic_clock::subscribe_duration(0);
        let instant_ready = instant_pollable.ready();
        duration_pollable.block();
        let clock_poll = wasi::io::poll::poll(&[&instant_pollable, &duration_pollable]);

        let input = wasi::cli::stdin::get_stdin();
        let read = input.read(1).unwrap();
        let blocking_read = input.blocking_read(1).unwrap();
        let skipped = input.skip(1).unwrap();
        let blocking_skipped = input.blocking_skip(1).unwrap();
        let input_pollable = input.subscribe();
        input_pollable.block();
        let input_ready = input_pollable.ready();

        let output = wasi::cli::stdout::get_stdout();
        let write_permit = output.check_write().unwrap();
        output.write(b"w").unwrap();
        output.blocking_write_and_flush(b"b").unwrap();
        output.blocking_write_zeroes_and_flush(1).unwrap();
        let output_pollable = output.subscribe();
        output_pollable.block();
        let output_ready = output_pollable.ready();
        output.write_zeroes(1).unwrap();
        output.flush().unwrap();
        output.blocking_flush().unwrap();
        let spliced = output.splice(&input, 1).unwrap();
        let blocking_spliced = output.blocking_splice(&input, 1).unwrap();

        let _ = wasi::cli::stderr::get_stderr();
        let terminal_stdin = wasi::cli::terminal_stdin::get_terminal_stdin().is_some();
        let terminal_stdout = wasi::cli::terminal_stdout::get_terminal_stdout().is_some();
        let terminal_stderr = wasi::cli::terminal_stderr::get_terminal_stderr().is_some();
        let random_bytes = wasi::random::random::get_random_bytes(4);
        let random_u64 = wasi::random::random::get_random_u64();
        let insecure_bytes = wasi::random::insecure::get_insecure_random_bytes(4);
        let insecure_u64 = wasi::random::insecure::get_insecure_random_u64();
        let insecure_seed = wasi::random::insecure_seed::insecure_seed();
        let filesystem = exercise_filesystem();
        let sockets = exercise_sockets();
        drop(input_pollable);
        drop(output_pollable);

        format!(
            "{environment:?}|{arguments:?}|{initial_cwd:?}|{}:{}|{}:{}|{instant}|{instant_resolution}|{instant_ready}|{clock_poll:?}|{read:?}|{blocking_read:?}|{skipped}|{blocking_skipped}|{input_ready}|{write_permit}|{output_ready}|{spliced}|{blocking_spliced}|{terminal_stdin}|{terminal_stdout}|{terminal_stderr}|random={random_bytes:?}:{random_u64}|insecure={insecure_bytes:?}:{insecure_u64}:{insecure_seed:?}|{filesystem}|{sockets}",
            wall.seconds, wall.nanoseconds, wall_resolution.seconds, wall_resolution.nanoseconds,
        )
    }

    fn exit_success() {
        wasi::cli::exit::exit(Ok(()));
    }

    fn exit_code() {
        wasi::cli::exit::exit_with_code(7);
    }

    fn common_calls() {
        let _ = wasi::cli::environment::get_environment();
        let _ = wasi::cli::environment::get_arguments();
        let _ = wasi::cli::terminal_stdin::get_terminal_stdin();
        let _ = wasi::cli::terminal_stdout::get_terminal_stdout();
        let _ = wasi::cli::terminal_stderr::get_terminal_stderr();
    }

    fn churn_files() {
        for _ in 0..128 {
            drop(std::fs::File::open("note.txt").unwrap());
        }
    }

    fn rust_std_paths() -> u32 {
        ["note.txt", "other.txt", "note.txt"]
            .into_iter()
            .map(|path| std::fs::read_to_string(path).unwrap().len() as u32)
            .sum()
    }

    fn quota_files() -> String {
        use wasi::filesystem::types::{DescriptorFlags, ErrorCode, OpenFlags, PathFlags};

        let directories = wasi::filesystem::preopens::get_directories();
        let root = &directories[0].0;
        let missing = matches!(
            root.open_at(
                PathFlags::empty(),
                "missing.txt",
                OpenFlags::empty(),
                DescriptorFlags::READ,
            ),
            Err(ErrorCode::NoEntry)
        );
        let note = root
            .open_at(
                PathFlags::empty(),
                "note.txt",
                OpenFlags::empty(),
                DescriptorFlags::READ,
            )
            .unwrap();
        let refused = matches!(
            root.open_at(
                PathFlags::empty(),
                "other.txt",
                OpenFlags::empty(),
                DescriptorFlags::READ,
            ),
            Err(ErrorCode::Access)
        );
        drop(note);
        let freed = root
            .open_at(
                PathFlags::empty(),
                "other.txt",
                OpenFlags::empty(),
                DescriptorFlags::READ,
            )
            .is_ok();

        format!("missing={missing}, refused={refused}, freed={freed}")
    }

    fn random_demo() {
        let first = wasi::random::random::get_random_u64() % 6 + 1;
        let second = wasi::random::random::get_random_u64() % 6 + 1;
        let bytes = wasi::random::random::get_random_bytes(4);
        let line = format!("dice: {first}, {second}; bytes: {bytes:?}\n");
        wasi::cli::stdout::get_stdout()
            .blocking_write_and_flush(line.as_bytes())
            .unwrap();
        let _ = wasi::random::random::get_random_u64();
    }

    fn socket_denial(port: u16) -> bool {
        use wasi::sockets::network::{
            ErrorCode, IpAddressFamily, IpSocketAddress, Ipv4SocketAddress,
        };

        let network = wasi::sockets::instance_network::instance_network();
        let socket =
            wasi::sockets::tcp_create_socket::create_tcp_socket(IpAddressFamily::Ipv4).unwrap();
        matches!(
            socket.start_connect(
                &network,
                IpSocketAddress::Ipv4(Ipv4SocketAddress {
                    address: (127, 0, 0, 1),
                    port,
                }),
            ),
            Err(ErrorCode::AccessDenied)
        )
    }

    fn net_allowlist(allowed_port: u16, denied_port: u16) -> String {
        use wasi::sockets::network::ErrorCode;

        let network = wasi::sockets::instance_network::instance_network();
        let address = resolve_localhost(&network);
        let allowed = connect_and_echo(&network, address, allowed_port)
            .unwrap_or_else(|error| format!("{error:?}"));
        let denied = connect_and_echo(&network, address, denied_port)
            .unwrap_or_else(|error| format!("{error:?}"));
        let denied = if denied == format!("{:?}", ErrorCode::AccessDenied) {
            "access-denied".to_owned()
        } else {
            denied
        };
        let udp_denied = send_datagram(&network, address, denied_port);
        let udp_allowed = send_datagram(&network, address, allowed_port);
        let udp_denied = match udp_denied {
            Err(ErrorCode::AccessDenied) => "access-denied".to_owned(),
            result => format!("{result:?}"),
        };
        let udp_allowed = match udp_allowed {
            Ok(1) => "delivered".to_owned(),
            result => format!("{result:?}"),
        };
        format!(
            "allowed: {allowed}\ndenied: {denied}\nudp allowed: {udp_allowed}\nudp denied: {udp_denied}\n"
        )
    }
}

export!(Component);

fn exercise_filesystem() -> String {
    use wasi::filesystem::types::{Advice, DescriptorFlags, NewTimestamp, OpenFlags, PathFlags};

    let directories = wasi::filesystem::preopens::get_directories();
    let root = &directories[0].0;
    let file = root
        .open_at(
            PathFlags::empty(),
            "note.txt",
            OpenFlags::empty(),
            DescriptorFlags::READ | DescriptorFlags::WRITE,
        )
        .unwrap();

    let _ = file.advise(0, 5, Advice::Sequential);
    let sync_data = file.sync_data().is_ok();
    let flags = file.get_flags().unwrap();
    let descriptor_type = file.get_type().unwrap();
    let _ = file.set_size(10);
    let _ = file.set_times(NewTimestamp::NoChange, NewTimestamp::NoChange);
    let direct = file.read(5, 0).unwrap().0;
    let _ = file.write(b"A", 9);
    let entries = root.read_directory().unwrap();
    while entries.read_directory_entry().unwrap().is_some() {}
    let sync = root.sync().is_ok();
    let created = root.create_directory_at("created").is_ok();
    let stat = file.stat().unwrap();
    let stat_at = root.stat_at(PathFlags::empty(), "note.txt").unwrap();
    let _ = root.set_times_at(
        PathFlags::empty(),
        "note.txt",
        NewTimestamp::NoChange,
        NewTimestamp::NoChange,
    );
    let linked = root
        .link_at(PathFlags::empty(), "note.txt", root, "linked.txt")
        .is_ok();
    let removed = root.remove_directory_at("empty-dir").is_ok();
    let renamed = root.rename_at("other.txt", root, "renamed.txt").is_ok();
    let symlinked = root.symlink_at("note.txt", "symbolic.txt").is_ok();
    let link_target = root.readlink_at("symbolic.txt").unwrap();
    let unlinked = root.unlink_file_at("linked.txt").is_ok();

    let input = file.read_via_stream(0).unwrap();
    let streamed = input.blocking_read(5).unwrap();
    let output = file.write_via_stream(5).unwrap();
    let _ = output.blocking_write_and_flush(b"write");
    let append = file.append_via_stream().unwrap();
    let _ = append.blocking_write_and_flush(b"append");
    let same = file.is_same_object(&file);
    let hash = file.metadata_hash().unwrap();
    let hash_at = root
        .metadata_hash_at(PathFlags::empty(), "note.txt")
        .unwrap();
    let hashes_match = hash.lower == hash_at.lower && hash.upper == hash_at.upper;
    let exists = |path| root.stat_at(PathFlags::empty(), path).is_ok();
    let effects = [
        created,
        exists("created"),
        removed,
        !exists("empty-dir"),
        linked,
        exists("note.txt"),
        renamed,
        !exists("other.txt"),
        exists("renamed.txt"),
        symlinked,
        link_target == "note.txt",
        unlinked,
        !exists("linked.txt"),
    ];

    format!(
        "{}:{}:{same}:{}:{}:{flags:?}:{descriptor_type:?}:{sync_data}:{sync}:{effects:?}:{hashes_match}",
        stat.size,
        stat_at.size,
        String::from_utf8_lossy(&direct),
        String::from_utf8_lossy(&streamed)
    )
}

fn exercise_sockets() -> String {
    use wasi::sockets::network::{
        ErrorCode, IpAddress, IpAddressFamily, IpSocketAddress, Ipv4SocketAddress,
    };
    use wasi::sockets::{instance_network, ip_name_lookup, tcp, tcp_create_socket, udp};

    let network = instance_network::instance_network();
    let listener = tcp_create_socket::create_tcp_socket(IpAddressFamily::Ipv4).unwrap();
    listener.set_listen_backlog_size(4).unwrap();
    listener
        .start_bind(
            &network,
            IpSocketAddress::Ipv4(Ipv4SocketAddress {
                address: (127, 0, 0, 1),
                port: 0,
            }),
        )
        .unwrap();
    listener.finish_bind().unwrap();
    let listen_address = listener.local_address().unwrap();
    listener.start_listen().unwrap();
    listener.finish_listen().unwrap();
    let listening = listener.is_listening();
    let tcp_family = listener.address_family();

    let client = tcp_create_socket::create_tcp_socket(IpAddressFamily::Ipv4).unwrap();
    let tcp_buffers = exercise_tcp_options(&client);
    client
        .start_bind(
            &network,
            IpSocketAddress::Ipv4(Ipv4SocketAddress {
                address: (127, 0, 0, 1),
                port: 0,
            }),
        )
        .unwrap();
    client.finish_bind().unwrap();
    client.start_connect(&network, listen_address).unwrap();
    let client_ready = client.subscribe();
    client_ready.block();
    let (client_input, client_output) = finish_connect(&client);
    let remote_matches = same_address(client.remote_address().unwrap(), listen_address);

    let listener_ready = listener.subscribe();
    listener_ready.block();
    let (accepted, accepted_input, accepted_output) = accept(&listener);
    let accepted_local = same_address(accepted.local_address().unwrap(), listen_address);
    let _ = accepted.remote_address().unwrap();
    client_output.blocking_write_and_flush(b"tcp").unwrap();
    let received = accepted_input.blocking_read(3).unwrap();
    accepted_output.blocking_write_and_flush(&received).unwrap();
    let echoed = client_input.blocking_read(3).unwrap();
    client.shutdown(tcp::ShutdownType::Both).unwrap();

    let udp_left = udp_create(&network);
    let udp_right = udp_create(&network);
    let udp_buffers = exercise_udp_options(&udp_left);
    let right_address = udp_right.local_address().unwrap();
    let (left_incoming, left_outgoing) = udp_left.stream(Some(right_address)).unwrap();
    let (right_incoming, right_outgoing) = udp_right.stream(None).unwrap();
    let udp_remote = same_address(udp_left.remote_address().unwrap(), right_address);
    let send_ready = left_outgoing.subscribe();
    while left_outgoing.check_send().unwrap() == 0 {
        send_ready.block();
    }
    let sent = left_outgoing
        .send(&[udp::OutgoingDatagram {
            data: b"udp".to_vec(),
            remote_address: None,
        }])
        .unwrap();
    let receive_ready = right_incoming.subscribe();
    let datagrams = loop {
        let datagrams = right_incoming.receive(1).unwrap();
        if !datagrams.is_empty() {
            break datagrams;
        }
        receive_ready.block();
    };

    let resolver = ip_name_lookup::resolve_addresses(&network, "localhost").unwrap();
    let resolver_ready = resolver.subscribe();
    let resolved = loop {
        match resolver.resolve_next_address() {
            Ok(Some(address)) => break address,
            Ok(None) => unreachable!(),
            Err(ErrorCode::WouldBlock) => resolver_ready.block(),
            Err(error) => panic!("localhost resolution failed: {error:?}"),
        }
    };
    while matches!(resolver.resolve_next_address(), Ok(Some(_))) {}

    drop(resolver_ready);
    drop(resolver);
    drop(send_ready);
    drop(receive_ready);
    drop(left_incoming);
    drop(left_outgoing);
    drop(right_incoming);
    drop(right_outgoing);
    drop(client_ready);
    drop(listener_ready);
    drop(client_input);
    drop(client_output);
    drop(accepted_input);
    drop(accepted_output);
    drop(accepted);
    drop(listener);
    drop(client);
    drop(network);

    format!(
        "sockets={listening}:{tcp_family:?}:{remote_matches}:{accepted_local}:{}:{tcp_buffers:?}:{sent}:{}:{udp_buffers:?}:{udp_remote}:{}",
        String::from_utf8_lossy(&echoed),
        String::from_utf8_lossy(&datagrams[0].data),
        matches!(resolved, IpAddress::Ipv4(_) | IpAddress::Ipv6(_)),
    )
}

fn finish_connect(
    socket: &wasi::sockets::tcp::TcpSocket,
) -> (
    wasi::io::streams::InputStream,
    wasi::io::streams::OutputStream,
) {
    loop {
        match socket.finish_connect() {
            Ok(streams) => return streams,
            Err(wasi::sockets::network::ErrorCode::WouldBlock) => socket.subscribe().block(),
            Err(error) => panic!("connect failed: {error:?}"),
        }
    }
}

fn accept(
    socket: &wasi::sockets::tcp::TcpSocket,
) -> (
    wasi::sockets::tcp::TcpSocket,
    wasi::io::streams::InputStream,
    wasi::io::streams::OutputStream,
) {
    loop {
        match socket.accept() {
            Ok(connection) => return connection,
            Err(wasi::sockets::network::ErrorCode::WouldBlock) => socket.subscribe().block(),
            Err(error) => panic!("accept failed: {error:?}"),
        }
    }
}

type TcpOptions = (bool, u64, u64, u32, u8);
type BufferSizes = (u64, u64);

fn exercise_tcp_options(
    socket: &wasi::sockets::tcp::TcpSocket,
) -> (TcpOptions, TcpOptions, BufferSizes) {
    let defaults = (
        socket.keep_alive_enabled().unwrap(),
        socket.keep_alive_idle_time().unwrap(),
        socket.keep_alive_interval().unwrap(),
        socket.keep_alive_count().unwrap(),
        socket.hop_limit().unwrap(),
    );
    socket.set_keep_alive_enabled(true).unwrap();
    socket.set_keep_alive_idle_time(13_000_000_000).unwrap();
    socket.set_keep_alive_interval(7_000_000_000).unwrap();
    socket.set_keep_alive_count(5).unwrap();
    socket.set_hop_limit(42).unwrap();
    socket.set_receive_buffer_size(8_192).unwrap();
    socket.set_send_buffer_size(32_768).unwrap();
    (
        defaults,
        (
            socket.keep_alive_enabled().unwrap(),
            socket.keep_alive_idle_time().unwrap(),
            socket.keep_alive_interval().unwrap(),
            socket.keep_alive_count().unwrap(),
            socket.hop_limit().unwrap(),
        ),
        (
            socket.receive_buffer_size().unwrap(),
            socket.send_buffer_size().unwrap(),
        ),
    )
}

fn udp_create(network: &wasi::sockets::network::Network) -> wasi::sockets::udp::UdpSocket {
    use wasi::sockets::network::{IpAddressFamily, IpSocketAddress, Ipv4SocketAddress};

    let socket =
        wasi::sockets::udp_create_socket::create_udp_socket(IpAddressFamily::Ipv4).unwrap();
    socket
        .start_bind(
            network,
            IpSocketAddress::Ipv4(Ipv4SocketAddress {
                address: (127, 0, 0, 1),
                port: 0,
            }),
        )
        .unwrap();
    socket.finish_bind().unwrap();
    socket
}

fn exercise_udp_options(socket: &wasi::sockets::udp::UdpSocket) -> (u8, u8, u64, u64) {
    let _ = socket.address_family();
    let default_hop_limit = socket.unicast_hop_limit().unwrap();
    socket.set_unicast_hop_limit(37).unwrap();
    socket.set_receive_buffer_size(8_192).unwrap();
    socket.set_send_buffer_size(32_768).unwrap();
    let options = (
        default_hop_limit,
        socket.unicast_hop_limit().unwrap(),
        socket.receive_buffer_size().unwrap(),
        socket.send_buffer_size().unwrap(),
    );
    let pollable = socket.subscribe();
    let _ = pollable.ready();
    options
}

fn same_address(
    left: wasi::sockets::network::IpSocketAddress,
    right: wasi::sockets::network::IpSocketAddress,
) -> bool {
    use wasi::sockets::network::IpSocketAddress;

    match (left, right) {
        (IpSocketAddress::Ipv4(left), IpSocketAddress::Ipv4(right)) => {
            left.address == right.address && left.port == right.port
        }
        (IpSocketAddress::Ipv6(left), IpSocketAddress::Ipv6(right)) => {
            left.address == right.address
                && left.port == right.port
                && left.flow_info == right.flow_info
                && left.scope_id == right.scope_id
        }
        (IpSocketAddress::Ipv4(_), IpSocketAddress::Ipv6(_))
        | (IpSocketAddress::Ipv6(_), IpSocketAddress::Ipv4(_)) => false,
    }
}

fn resolve_localhost(
    network: &wasi::sockets::network::Network,
) -> wasi::sockets::network::Ipv4Address {
    use wasi::sockets::network::{ErrorCode, IpAddress};

    let resolver = wasi::sockets::ip_name_lookup::resolve_addresses(network, "localhost").unwrap();
    let ready = resolver.subscribe();
    loop {
        match resolver.resolve_next_address() {
            Ok(Some(IpAddress::Ipv4(address))) => return address,
            Ok(Some(IpAddress::Ipv6(_))) => {}
            Ok(None) => panic!("localhost did not resolve to IPv4"),
            Err(ErrorCode::WouldBlock) => {
                assert!(wait_before_socket_timeout(&ready), "DNS lookup timed out");
            }
            Err(error) => panic!("localhost resolution failed: {error:?}"),
        }
    }
}

fn wait_before_socket_timeout(pollable: &wasi::io::poll::Pollable) -> bool {
    let timeout = wasi::clocks::monotonic_clock::subscribe_duration(10_000_000_000);
    wasi::io::poll::poll(&[pollable, &timeout]).contains(&0)
}

fn connect_and_echo(
    network: &wasi::sockets::network::Network,
    address: wasi::sockets::network::Ipv4Address,
    port: u16,
) -> Result<String, wasi::sockets::network::ErrorCode> {
    use wasi::sockets::network::{IpAddressFamily, IpSocketAddress, Ipv4SocketAddress};

    let socket = wasi::sockets::tcp_create_socket::create_tcp_socket(IpAddressFamily::Ipv4)?;
    socket.start_bind(
        network,
        IpSocketAddress::Ipv4(Ipv4SocketAddress {
            address: (127, 0, 0, 1),
            port: 0,
        }),
    )?;
    socket.finish_bind()?;
    socket.start_connect(
        network,
        IpSocketAddress::Ipv4(Ipv4SocketAddress { address, port }),
    )?;
    let connected = socket.subscribe();
    if !wait_before_socket_timeout(&connected) {
        return Err(wasi::sockets::network::ErrorCode::Timeout);
    }
    let (input, output) = finish_connect(&socket);
    let writable = output.subscribe();
    while output.check_write().unwrap() < 5 {
        if !wait_before_socket_timeout(&writable) {
            return Err(wasi::sockets::network::ErrorCode::Timeout);
        }
    }
    output.write(b"hello").unwrap();
    output.flush().unwrap();
    if !wait_before_socket_timeout(&writable) {
        return Err(wasi::sockets::network::ErrorCode::Timeout);
    }
    let readable = input.subscribe();
    if !wait_before_socket_timeout(&readable) {
        return Err(wasi::sockets::network::ErrorCode::Timeout);
    }
    let echo = input.read(5).unwrap();
    Ok(String::from_utf8_lossy(&echo).into_owned())
}

fn send_datagram(
    network: &wasi::sockets::network::Network,
    address: wasi::sockets::network::Ipv4Address,
    port: u16,
) -> Result<u64, wasi::sockets::network::ErrorCode> {
    use wasi::sockets::network::{IpAddressFamily, IpSocketAddress, Ipv4SocketAddress};
    use wasi::sockets::udp::OutgoingDatagram;

    let socket = wasi::sockets::udp_create_socket::create_udp_socket(IpAddressFamily::Ipv4)?;
    socket.start_bind(
        network,
        IpSocketAddress::Ipv4(Ipv4SocketAddress {
            address: (127, 0, 0, 1),
            port: 0,
        }),
    )?;
    socket.finish_bind()?;
    let (_, outgoing) = socket.stream(None)?;
    let ready = outgoing.subscribe();
    while outgoing.check_send()? == 0 {
        if !wait_before_socket_timeout(&ready) {
            return Err(wasi::sockets::network::ErrorCode::Timeout);
        }
    }
    outgoing.send(&[OutgoingDatagram {
        data: b"udp".to_vec(),
        remote_address: Some(IpSocketAddress::Ipv4(Ipv4SocketAddress { address, port })),
    }])
}
