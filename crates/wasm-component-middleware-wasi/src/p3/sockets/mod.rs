mod ip_name_lookup;
mod tcp;
mod udp;

use wasm_component_middleware::ArgumentValue;
use wasmtime_wasi::p3::bindings::sockets::types;
use wasmtime_wasi::p3::bindings::sockets::types::IpSocketAddress;

use crate::gate::{GateData, project};

use super::relay::Relayed;

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

pub(super) fn add_to_linker<T>(linker: &mut wasmtime::component::Linker<T>) -> wasmtime::Result<()>
where
    T: wasmtime_wasi::WasiView + wasm_component_middleware::MiddlewareView + 'static,
{
    types::add_to_linker::<T, GateData<T>>(linker, project::<T>)?;
    ip_name_lookup::add_to_linker(linker)
}

pub(super) fn add_to_linker_relayed<T, const CAPACITY: usize>(
    linker: &mut wasmtime::component::Linker<T>,
) -> wasmtime::Result<()>
where
    T: wasmtime_wasi::WasiView + wasm_component_middleware::MiddlewareView + 'static,
{
    types::add_to_linker::<T, GateData<T, Relayed<CAPACITY>>>(linker, project::<T>)?;
    ip_name_lookup::add_to_linker(linker)
}
