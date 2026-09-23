mod ip_name_lookup;
mod network;
mod tcp;
mod udp;

use wasm_component_middleware::ArgumentValue;
use wasmtime_wasi::p2::bindings::sockets::network::IpSocketAddress;
use wasmtime_wasi::p2::bindings::sync::sockets::udp::OutgoingDatagram;

pub(super) use network::add_to_linker as add_link_options_interfaces_to_linker;

pub(super) fn add_to_linker<T>(linker: &mut wasmtime::component::Linker<T>) -> wasmtime::Result<()>
where
    T: wasmtime_wasi::WasiView + wasm_component_middleware::MiddlewareView + 'static,
{
    tcp::add_to_linker(linker)?;
    udp::add_to_linker(linker)?;
    ip_name_lookup::add_to_linker(linker)
}

fn address(address: IpSocketAddress) -> String {
    std::net::SocketAddr::from(address).to_string()
}

fn optional_address(value: Option<IpSocketAddress>) -> ArgumentValue {
    value.map_or(
        ArgumentValue::Variant {
            case: "none",
            value: None,
        },
        |value| ArgumentValue::Variant {
            case: "some",
            value: Some(Box::new(address(value).into())),
        },
    )
}

fn datagrams(datagrams: &[OutgoingDatagram]) -> ArgumentValue {
    ArgumentValue::List(
        datagrams
            .iter()
            .map(|datagram| {
                ArgumentValue::List(vec![
                    ArgumentValue::bytes(&datagram.data),
                    optional_address(datagram.remote_address),
                ])
            })
            .collect(),
    )
}
