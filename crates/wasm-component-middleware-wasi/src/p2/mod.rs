mod cli;
mod clocks;
mod filesystem;
mod gate;
mod io;
mod random;
mod sockets;

pub(super) const WASI_VERSION: &str = "0.2.12";

use wasm_component_middleware::MiddlewareView;
use wasm_component_middleware::RoutedInterface;
use wasmtime::component::Linker;
use wasmtime_wasi::WasiView;

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
    RoutedInterface::from_static("wasi:filesystem/preopens"),
    RoutedInterface::from_static("wasi:filesystem/types"),
    RoutedInterface::from_static("wasi:io/error"),
    RoutedInterface::from_static("wasi:io/poll"),
    RoutedInterface::from_static("wasi:io/streams"),
    RoutedInterface::from_static("wasi:random/insecure"),
    RoutedInterface::from_static("wasi:random/insecure-seed"),
    RoutedInterface::from_static("wasi:random/random"),
    RoutedInterface::from_static("wasi:sockets/instance-network"),
    RoutedInterface::from_static("wasi:sockets/ip-name-lookup"),
    RoutedInterface::from_static("wasi:sockets/network"),
    RoutedInterface::from_static("wasi:sockets/tcp"),
    RoutedInterface::from_static("wasi:sockets/tcp-create-socket"),
    RoutedInterface::from_static("wasi:sockets/udp"),
    RoutedInterface::from_static("wasi:sockets/udp-create-socket"),
];

/// Adds synchronous WASI Preview 2 interfaces with middleware gates.
///
/// This mirrors [`wasmtime_wasi::p2::add_to_linker_sync`], routing every
/// `wasi:cli`, `wasi:clocks`, `wasi:filesystem`, `wasi:io`, and `wasi:random`
/// calls through the store's [`MiddlewareView`].
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker_sync<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    add_to_linker_with_options_sync(
        linker,
        &wasmtime_wasi::p2::bindings::sync::LinkOptions::default(),
    )
}

/// Adds synchronous WASI Preview 2 interfaces with middleware gates and options.
///
/// Use this when enabling unstable WASI functions through Wasmtime's
/// [`wasmtime_wasi::p2::bindings::sync::LinkOptions`].
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker_with_options_sync<T>(
    linker: &mut Linker<T>,
    options: &wasmtime_wasi::p2::bindings::sync::LinkOptions,
) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    sockets::add_link_options_interfaces_to_linker::<T>(linker, options)?;
    cli::add_to_linker::<T>(linker)?;
    clocks::add_to_linker::<T>(linker)?;
    filesystem::add_to_linker::<T>(linker)?;
    io::add_to_linker::<T>(linker)?;
    random::add_to_linker::<T>(linker)?;
    sockets::add_to_linker::<T>(linker)
}
