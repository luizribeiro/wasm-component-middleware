//! Middleware for calls across a WebAssembly component boundary.
//!
//! Use this crate to layer host-defined policy and observation over component
//! imports and exports without bringing in WASI-specific dependencies.

mod async_dispatch;
mod chain;
mod context;
mod types;

pub use async_dispatch::StateAccess;
pub use chain::{Chain, ChainBuilder};
pub use context::{InvocationContext, MiddlewareCtx, MiddlewareView};
pub use types::{Call, Completion, Denied, Direction, Layer, Outcome};
