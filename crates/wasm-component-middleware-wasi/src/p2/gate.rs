use std::marker::PhantomData;

use wasmtime::component::HasData;

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
        handles = [$($handle:expr),* $(,)?], args = $args:expr,
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
        handles = [$($handle:expr),* $(,)?], args = $args:expr,
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
        handles = $resources:expr, args = $args:expr,
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
        handles = $handles:expr, args = $args:expr,
        delegate = $delegate:expr, produced = $produced:expr
    ) => {{
        let chain = std::sync::Arc::clone($gate.state.middleware().chain());
        let handles: Vec<u32> = ($handles).into_iter().collect();
        let arguments = $args;
        let call = wasm_component_middleware::Call {
            id: chain.next_id(),
            direction: wasm_component_middleware::Direction::Import,
            interface: Some($interface),
            version: Some("0.2.12"),
            function: $function,
            handles: &handles,
            args: &arguments,
        };
        chain.dispatch($gate.state, &call, |state| {
            let value = ($delegate)(state)?;
            let produced = ($produced)(&value);
            Ok((value, wasm_component_middleware::Completion { produced }))
        })
    }};
    (
        stream $gate:ident, $interface:literal, $function:literal,
        handles = [$($handle:expr),* $(,)?], args = $args:expr,
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
}

pub(super) use gate;
