//! Middleware gates for concurrent WASI Preview 3 interfaces.

mod cli;
mod clocks;

use wasm_component_middleware::{MiddlewareView, RoutedInterface};
use wasmtime::component::Linker;
use wasmtime_wasi::WasiView;

pub(super) const WASI_VERSION: &str = "0.3.0";

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
];

/// Adds WASI Preview 3 interfaces with middleware gates.
///
/// This mirrors [`wasmtime_wasi::p3::add_to_linker`], routing every
/// `wasi:cli` and `wasi:clocks` call through the store's [`MiddlewareView`].
///
/// # Errors
///
/// Returns an error if Wasmtime cannot register an interface.
pub fn add_to_linker<T>(linker: &mut Linker<T>) -> wasmtime::Result<()>
where
    T: WasiView + MiddlewareView + 'static,
{
    wasmtime_wasi::p3::filesystem::add_to_linker(linker)?;
    wasmtime_wasi::p3::random::add_to_linker(linker)?;
    wasmtime_wasi::p3::sockets::add_to_linker(linker)?;
    cli::add_to_linker(linker)?;
    clocks::add_to_linker(linker)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use wit_parser::Resolve;

    use super::WASI_VERSION;

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
