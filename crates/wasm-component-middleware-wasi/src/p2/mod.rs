mod cli;
mod clocks;
mod gate;
mod io;

pub(super) const WASI_VERSION: &str = "0.2.12";

use wasm_component_middleware::MiddlewareView;
use wasm_component_middleware::RoutedInterface;
use wasmtime::component::Linker;
use wasmtime_wasi::WasiView;
use wasmtime_wasi::filesystem::{WasiFilesystem, WasiFilesystemView as _};
use wasmtime_wasi::random::WasiRandom;
use wasmtime_wasi::sockets::{WasiSockets, WasiSocketsView as _};

/// Interfaces routed by [`add_to_linker_sync`].
///
/// Pass these markers to [`wasm_component_middleware::verify_routing`] along
/// with markers for application-defined imports.
pub const ROUTED_INTERFACES: &[RoutedInterface] = &[
    RoutedInterface::from_static("wasi:cli/environment"),
    RoutedInterface::from_static("wasi:cli/exit"),
    RoutedInterface::from_static("wasi:cli/stderr"),
    RoutedInterface::from_static("wasi:cli/stdin"),
    RoutedInterface::from_static("wasi:cli/stdout"),
    RoutedInterface::from_static("wasi:cli/terminal-input"),
    RoutedInterface::from_static("wasi:cli/terminal-output"),
    RoutedInterface::from_static("wasi:cli/terminal-stderr"),
    RoutedInterface::from_static("wasi:cli/terminal-stdin"),
    RoutedInterface::from_static("wasi:cli/terminal-stdout"),
    RoutedInterface::from_static("wasi:clocks/monotonic-clock"),
    RoutedInterface::from_static("wasi:clocks/wall-clock"),
    RoutedInterface::from_static("wasi:io/error"),
    RoutedInterface::from_static("wasi:io/poll"),
    RoutedInterface::from_static("wasi:io/streams"),
];

/// Adds synchronous WASI Preview 2 interfaces with middleware gates.
///
/// This mirrors [`wasmtime_wasi::p2::add_to_linker_sync`], routing every
/// `wasi:cli`, `wasi:clocks`, and `wasi:io` call through the store's
/// [`MiddlewareView`].
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker_sync<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    add_ungated_to_linker_sync(linker)?;
    cli::add_to_linker::<T>(linker)?;
    clocks::add_to_linker::<T>(linker)?;
    io::add_to_linker::<T>(linker)
}

fn add_ungated_to_linker_sync<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + 'static,
{
    use wasmtime_wasi::p2::bindings::{filesystem, random, sockets};

    let options = wasmtime_wasi::p2::bindings::sync::LinkOptions::default();
    filesystem::preopens::add_to_linker::<T, WasiFilesystem>(linker, T::filesystem)?;
    random::random::add_to_linker::<T, WasiRandom>(linker, |state| state.ctx().ctx.random())?;
    random::insecure::add_to_linker::<T, WasiRandom>(linker, |state| state.ctx().ctx.random())?;
    random::insecure_seed::add_to_linker::<T, WasiRandom>(linker, |state| {
        state.ctx().ctx.random()
    })?;
    sockets::tcp_create_socket::add_to_linker::<T, WasiSockets>(linker, T::sockets)?;
    sockets::instance_network::add_to_linker::<T, WasiSockets>(linker, T::sockets)?;
    sockets::network::add_to_linker::<T, WasiSockets>(linker, &(&options).into(), T::sockets)?;
    wasmtime_wasi::p2::bindings::sync::filesystem::types::add_to_linker::<T, WasiFilesystem>(
        linker,
        T::filesystem,
    )?;
    wasmtime_wasi::p2::bindings::sync::sockets::tcp::add_to_linker::<T, WasiSockets>(
        linker,
        T::sockets,
    )?;
    wasmtime_wasi::p2::bindings::sync::sockets::udp::add_to_linker::<T, WasiSockets>(
        linker,
        T::sockets,
    )?;
    wasmtime_wasi::p2::bindings::sync::sockets::udp_create_socket::add_to_linker::<T, WasiSockets>(
        linker,
        T::sockets,
    )?;
    wasmtime_wasi::p2::bindings::sync::sockets::ip_name_lookup::add_to_linker::<T, WasiSockets>(
        linker,
        T::sockets,
    )
}
