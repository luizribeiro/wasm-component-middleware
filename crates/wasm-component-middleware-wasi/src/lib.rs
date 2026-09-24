//! Middleware gates for WASI Preview 2 and Preview 3 interfaces.
//!
//! Use this crate when an embedder needs to select, observe, or restrict the
//! WASI interfaces exposed to a component.

mod gate;
mod open_files;
/// Middleware gates for synchronous WASI Preview 2 interfaces.
pub mod p2;
pub mod p3;

pub use open_files::OpenFiles;

use wasm_component_middleware::{RoutedInterface, Unrouted};
use wasmtime::Engine;
use wasmtime::component::Component;

/// Verifies that every component import is routed through middleware.
///
/// WASI Preview 2 and Preview 3 interfaces are included automatically. Pass
/// markers for application-defined and extension-package imports in `routed`.
/// Unlike the lower-level [`wasm_component_middleware::verify_routing`], this
/// check has no unchecked prefixes.
///
/// # Errors
///
/// Returns an error naming every import that is neither a routed WASI
/// interface nor present in `routed`.
pub fn verify_routing(
    engine: &Engine,
    component: &Component,
    routed: impl IntoIterator<Item = RoutedInterface>,
) -> Result<(), Unrouted> {
    wasm_component_middleware::verify_routing(
        engine,
        component,
        p2::ROUTED_INTERFACES
            .iter()
            .chain(p3::ROUTED_INTERFACES)
            .copied()
            .chain(routed),
        [],
    )
}
