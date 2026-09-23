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
