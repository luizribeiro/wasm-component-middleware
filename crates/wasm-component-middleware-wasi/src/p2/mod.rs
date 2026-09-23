mod clocks;
mod gate;

use wasm_component_middleware::MiddlewareView;
use wasm_component_middleware::RoutedInterface;
use wasmtime::component::Linker;
use wasmtime_wasi::WasiView;

/// Interfaces routed by [`add_to_linker_sync`].
///
/// Pass these markers to [`wasm_component_middleware::verify_routing`] along
/// with markers for application-defined imports.
pub const ROUTED_INTERFACES: &[RoutedInterface] = &[
    RoutedInterface::from_static("wasi:clocks/monotonic-clock"),
    RoutedInterface::from_static("wasi:clocks/wall-clock"),
];

/// Adds synchronous WASI Preview 2 interfaces with middleware gates.
///
/// This mirrors [`wasmtime_wasi::p2::add_to_linker_sync`], routing every
/// `wasi:clocks` call through the store's [`MiddlewareView`]. Wasmtime does not
/// expose the linker's current shadowing setting, so this function enables
/// shadowing for gate installation and leaves it enabled.
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker_sync<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    wasmtime_wasi::p2::add_to_linker_sync(linker)?;
    linker.allow_shadowing(true);
    clocks::add_to_linker::<T>(linker)
}
