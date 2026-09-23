use std::marker::PhantomData;

use wasmtime::component::HasData;

pub(super) const WASI_VERSION: &str = "0.2.12";

pub(super) struct Gate<'a, T> {
    pub(super) state: &'a mut T,
}

pub(super) fn project<T>(state: &mut T) -> Gate<'_, T> {
    Gate { state }
}

pub(super) struct GateData<T>(PhantomData<fn() -> T>);

impl<T: 'static> HasData for GateData<T> {
    type Data<'a> = Gate<'a, T>;
}

macro_rules! gate {
    (
        trap $gate:ident, $interface:literal, $function:literal,
        handles = [$($handle:expr),* $(,)?], args = $args:tt,
        delegate = $delegate:expr
    ) => {{
        gate!(
            @trap $gate, $interface, $function,
            handles = [$($handle.rep()),*], args = $args,
            delegate = $delegate, produced = |_| Vec::<u32>::new()
        )
    }};
    (
        trap $gate:ident, $interface:literal, $function:literal,
        handles = [$($handle:expr),* $(,)?], args = $args:tt,
        delegate = $delegate:expr, produced = $produced:expr
    ) => {{
        gate!(
            @trap $gate, $interface, $function,
            handles = [$($handle.rep()),*], args = $args,
            delegate = $delegate, produced = $produced
        )
    }};
    (
        trap_each $gate:ident, $interface:literal, $function:literal,
        handles = $resources:expr, args = $args:tt,
        delegate = $delegate:expr
    ) => {{
        gate!(
            @trap $gate, $interface, $function,
            handles = $resources.iter().map(wasmtime::component::Resource::rep), args = $args,
            delegate = $delegate, produced = |_| Vec::<u32>::new()
        )
    }};
    (
        @trap $gate:ident, $interface:literal, $function:literal,
        handles = $handles:expr, args = $args:tt,
        delegate = $delegate:expr, produced = $produced:expr
    ) => {{
        let chain = std::sync::Arc::clone($gate.state.middleware().chain());
        let handles: Vec<u32> = ($handles).into_iter().collect();
        let arguments = gate!(@args $args);
        let call = wasm_component_middleware::Call::new(
            chain.next_id(),
            wasm_component_middleware::Direction::Import,
            $function,
        )
        .in_interface($interface, Some($crate::p2::gate::WASI_VERSION))
        .with_handles(&handles)
        .with_args(&arguments);
        chain.dispatch($gate.state, &call, |state| {
            let value = ($delegate)(state)?;
            let produced = ($produced)(&value);
            Ok((value, wasm_component_middleware::Completion { produced }))
        })
    }};
    (
        stream $gate:ident, $interface:literal, $function:literal,
        handles = [$($handle:expr),* $(,)?], args = $args:tt,
        delegate = $delegate:expr
    ) => {{
        let result = gate!(
            trap $gate, $interface, $function,
            handles = [$($handle),*], args = $args,
            delegate = |state| Ok::<_, wasmtime::Error>(($delegate)(state))
        );
        result.unwrap_or_else(|error| match error.downcast::<wasm_component_middleware::Denied>() {
            Ok(denied) => Err(wasmtime_wasi::p2::StreamError::LastOperationFailed(denied.into())),
            Err(error) => Err(wasmtime_wasi::p2::StreamError::Trap(error)),
        })
    }};
    (@args ()) => {
        wasm_component_middleware::Arguments::new()
    };
    (@args [$($name:ident = $value:expr),* $(,)?]) => {
        wasm_component_middleware::Arguments::new()
            $(.with(stringify!($name), $value))*
    };
    (@args ($($value:expr),+ $(,)?)) => {
        wasm_component_middleware::Arguments::new()
            $(.with_debug(stringify!($value), &$value))*
    };
}

pub(super) use gate;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use wit_parser::Resolve;

    use super::WASI_VERSION;

    #[test]
    fn reported_version_matches_every_vendored_dependency() {
        let wit = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("wit");
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
