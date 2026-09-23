//! Middleware for calls across a WebAssembly component boundary.
//!
//! Use this crate to layer host-defined policy and observation over component
//! imports and exports without bringing in WASI-specific dependencies.

mod allowlist;
mod async_dispatch;
mod chain;
mod context;
mod logger;
mod routing;
mod types;
mod verification;

pub use allowlist::Allowlist;
pub use async_dispatch::StateAccess;
pub use chain::{Chain, ChainBuilder};
pub use context::{InvocationContext, MiddlewareCtx, MiddlewareView};
pub use logger::Logger;
pub use routing::{Routed, RoutedInterface, Routing, dispatch_import, route_export};
pub use types::{
    ArgumentValue, Arguments, BYTE_ARGUMENT_PREFIX_LEN, Call, Completion, Denied, Direction, Layer,
    Outcome,
};
pub use verification::{Unrouted, verify_routing};
