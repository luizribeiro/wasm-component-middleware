//! Middleware for calls across a WebAssembly component boundary.
//!
//! Use this crate to layer host-defined policy and observation over component
//! imports and exports without bringing in WASI-specific dependencies.

mod types;

pub use types::{Call, Completion, Denied, Direction, Layer, Outcome};
