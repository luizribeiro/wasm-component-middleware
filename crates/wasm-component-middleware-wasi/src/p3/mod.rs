//! Middleware gates for concurrent WASI Preview 3 interfaces.

mod cli;
mod clocks;
mod filesystem;
mod random;
mod relay;
mod sockets;

use wasm_component_middleware::{MiddlewareView, RoutedInterface};
use wasmtime::component::Linker;
use wasmtime_wasi::WasiView;

pub(super) const WASI_VERSION: &str = "0.3.0";

/// Default maximum number of bytes buffered by each relayed byte stream.
pub const DEFAULT_STREAM_BUFFER_CAPACITY: usize = 64 * 1024;

/// Configuration for relayed byte streams on one interface.
///
/// Middleware sees the complete bytes of each chunk before the relay
/// acknowledges them to the source. When a layer denies a chunk, that chunk
/// remains unacknowledged, previously approved chunks drain to the destination,
/// and the replacement completion future reports the denial. Relayed streams
/// use [`wasmtime::component::StreamProducer`]'s default `try_into` behavior so
/// a host-to-host short circuit cannot bypass the chain; unrelayed interfaces
/// retain the original producer and its conversion behavior.
#[derive(Clone, Copy)]
pub struct StreamRelay<const CAPACITY: usize = DEFAULT_STREAM_BUFFER_CAPACITY>(());

impl Default for StreamRelay<DEFAULT_STREAM_BUFFER_CAPACITY> {
    fn default() -> Self {
        Self(())
    }
}

impl<const CAPACITY: usize> StreamRelay<CAPACITY> {
    /// Creates relay options when `CAPACITY` is nonzero.
    #[must_use]
    pub const fn new() -> Option<Self> {
        if CAPACITY == 0 { None } else { Some(Self(())) }
    }
}

/// Interfaces routed by [`add_to_linker`].
pub const ROUTED_INTERFACES: &[RoutedInterface] = &[
    RoutedInterface::from_static("wasi:cli/environment"),
    RoutedInterface::from_static("wasi:cli/exit"),
    RoutedInterface::from_static("wasi:cli/types"),
    RoutedInterface::from_static("wasi:cli/stdin"),
    RoutedInterface::from_static("wasi:cli/stdout"),
    RoutedInterface::from_static("wasi:cli/stderr"),
    RoutedInterface::from_static("wasi:cli/terminal-input"),
    RoutedInterface::from_static("wasi:cli/terminal-output"),
    RoutedInterface::from_static("wasi:cli/terminal-stdin"),
    RoutedInterface::from_static("wasi:cli/terminal-stdout"),
    RoutedInterface::from_static("wasi:cli/terminal-stderr"),
    RoutedInterface::from_static("wasi:clocks/types"),
    RoutedInterface::from_static("wasi:clocks/monotonic-clock"),
    RoutedInterface::from_static("wasi:clocks/system-clock"),
    RoutedInterface::from_static("wasi:filesystem/preopens"),
    RoutedInterface::from_static("wasi:filesystem/types"),
    RoutedInterface::from_static("wasi:random/insecure"),
    RoutedInterface::from_static("wasi:random/insecure-seed"),
    RoutedInterface::from_static("wasi:random/random"),
    RoutedInterface::from_static("wasi:sockets/ip-name-lookup"),
    RoutedInterface::from_static("wasi:sockets/types"),
];

/// Adds WASI Preview 3 interfaces with middleware gates.
///
/// This mirrors [`wasmtime_wasi::p3::add_to_linker`], routing every
/// `wasi:cli`, `wasi:clocks`, `wasi:filesystem`, and `wasi:random` calls
/// through the store's [`MiddlewareView`].
/// Filesystem stream calls are gated, but their bytes pass through without
/// relay or observation.
/// Refusing filesystem `read-via-stream`, `write-via-stream`,
/// `append-via-stream`, or `read-directory` traps because Preview 3 gives
/// those functions no top-level error result in which to return a refusal.
/// Wasmtime produces accepted Preview 3 TCP sockets inside the stream returned
/// by `tcp-socket.listen`, so layers see the `listen` call but not each accepted
/// socket. Preview 2 `tcp-socket.accept` remains individually visible.
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    add_to_linker_with_options(linker, &wasmtime_wasi::p3::bindings::LinkOptions::default())
}

/// Adds Preview 3 interfaces with middleware gates and linker options.
///
/// This mirrors [`wasmtime_wasi::p3::add_to_linker_with_options`].
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker_with_options<T>(
    linker: &mut Linker<T>,
    _options: &wasmtime_wasi::p3::bindings::LinkOptions,
) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    filesystem::add_to_linker(linker)?;
    random::add_to_linker(linker)?;
    sockets::add_to_linker(linker)?;
    cli::add_to_linker(linker)?;
    clocks::add_to_linker(linker)
}

/// Adds Preview 3 interfaces and relays filesystem, socket, and stdio byte streams.
///
/// Each transferred chunk appears as a synthetic call carrying the opening
/// call's identifier and any resource handles. Use [`add_to_linker`] when
/// byte-level policy is unnecessary and the direct Wasmtime path is preferred.
/// Wasmtime produces accepted Preview 3 TCP sockets inside the stream returned
/// by `tcp-socket.listen`, so layers see the `listen` call but not each accepted
/// socket. Preview 2 `tcp-socket.accept` remains individually visible.
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker_with_stream_relay<T, const CAPACITY: usize>(
    linker: &mut Linker<T>,
    _relay: StreamRelay<CAPACITY>,
) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    filesystem::add_to_linker_relayed::<T, CAPACITY>(linker)?;
    random::add_to_linker(linker)?;
    sockets::add_to_linker_relayed::<T, CAPACITY>(linker)?;
    cli::add_to_linker_relayed::<T, CAPACITY>(linker)?;
    clocks::add_to_linker(linker)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use wit_parser::Resolve;

    use super::WASI_VERSION;

    #[test]
    fn stream_relay_requires_a_nonzero_bound() {
        assert!(super::StreamRelay::<0>::new().is_none());
        assert!(super::StreamRelay::<8192>::new().is_some());
    }

    #[test]
    fn reported_version_matches_every_vendored_dependency() {
        let wit = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wit-p3");
        let dependency_count = fs::read_dir(wit.join("deps"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "wit")
            })
            .count();
        let mut resolve = Resolve::default();
        resolve.push_dir(wit).unwrap();
        let versions = resolve
            .packages
            .iter()
            .filter_map(|(_, package)| package.name.version.as_ref())
            .collect::<Vec<_>>();

        assert_eq!(versions.len(), dependency_count);
        assert!(
            versions
                .iter()
                .all(|version| version.to_string() == WASI_VERSION)
        );
    }
}
