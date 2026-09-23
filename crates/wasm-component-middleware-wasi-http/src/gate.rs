use std::marker::PhantomData;

use wasmtime::component::HasData;

pub(crate) struct Gate<'a, T> {
    pub(crate) state: &'a mut T,
}

pub(crate) fn project<T>(state: &mut T) -> Gate<'_, T> {
    Gate { state }
}

pub(crate) struct GateData<T>(PhantomData<fn() -> T>);

impl<T: 'static> HasData for GateData<T> {
    type Data<'a> = Gate<'a, T>;
}

pub(crate) fn trappable_error<T, E>(
    error: E,
    downcast: impl FnOnce(E) -> wasmtime::Result<T>,
) -> wasmtime::Error
where
    T: std::error::Error + Send + Sync + 'static,
{
    match downcast(error) {
        Ok(code) => wasmtime::Error::new(code),
        Err(error) => error,
    }
}

pub(crate) fn typed_error<T, E>(
    error: wasmtime::Error,
    trap: impl FnOnce(wasmtime::Error) -> E,
) -> E
where
    T: std::error::Error + Send + Sync + 'static,
    E: From<T>,
{
    match error.downcast::<T>() {
        Ok(code) => code.into(),
        Err(error) => trap(error),
    }
}

macro_rules! gate {
    (
        access $store:ident, $version:expr, $interface:expr, $function:literal,
        handles = $handles:expr, args = $args:tt, delegate = $delegate:expr
        $(, produced = $produced:expr)?
    ) => {{
        let chain = std::sync::Arc::clone($store.data_mut().middleware().chain());
        let handles: Vec<u32> = ($handles).into_iter().collect();
        let arguments = gate!(@args $args);
        let call = wasm_component_middleware::Call::new(
            chain.next_id(),
            wasm_component_middleware::Direction::Import,
            $function,
        )
        .in_interface($interface, Some($version))
        .with_handles(&handles)
        .with_args(&arguments);
        chain.dispatch_access($store, &call, |store| {
            let value = ($delegate)(store)?;
            let produced = gate!(@produced value $(, $produced)?);
            Ok((value, wasm_component_middleware::Completion { produced }))
        })
    }};
    (
        async $store:ident, $version:expr, $interface:expr, $function:literal,
        handles = $handles:expr, args = $args:tt, delegate = $delegate:expr
        $(, produced = $produced:expr)?
    ) => {{
        let chain = $store.with(|mut access| {
            std::sync::Arc::clone(access.data_mut().middleware().chain())
        });
        let handles: Vec<u32> = ($handles).into_iter().collect();
        let arguments = gate!(@args $args);
        let call = wasm_component_middleware::Call::new(
            chain.next_id(),
            wasm_component_middleware::Direction::Import,
            $function,
        )
        .in_interface($interface, Some($version))
        .with_handles(&handles)
        .with_args(&arguments);
        chain.dispatch_async($store, &call, || async {
            let value = ($delegate).await?;
            let produced = gate!(@produced value $(, $produced)?);
            Ok((value, wasm_component_middleware::Completion { produced }))
        }).await
    }};
    (
        trap $gate:ident, $version:expr, $interface:expr, $function:literal,
        handles = [$($handle:expr),* $(,)?], args = $args:tt,
        delegate = $delegate:expr $(, produced = $produced:expr)?
    ) => {{
        let chain = std::sync::Arc::clone($gate.state.middleware().chain());
        let handles: Vec<u32> = [$($handle.rep()),*].into_iter().collect();
        let arguments = gate!(@args $args);
        let call = wasm_component_middleware::Call::new(
            chain.next_id(),
            wasm_component_middleware::Direction::Import,
            $function,
        )
        .in_interface($interface, Some($version))
        .with_handles(&handles)
        .with_args(&arguments);
        chain.dispatch($gate.state, &call, |state| {
            let value = ($delegate)(state)?;
            let produced = gate!(@produced value $(, $produced)?);
            Ok((value, wasm_component_middleware::Completion { produced }))
        })
    }};
    (
        trap_reps $gate:ident, $version:expr, $interface:expr, $function:literal,
        handles = $handles:expr, args = $args:tt,
        delegate = $delegate:expr $(, produced = $produced:expr)?
    ) => {{
        let chain = std::sync::Arc::clone($gate.state.middleware().chain());
        let handles: Vec<u32> = ($handles).into_iter().collect();
        let arguments = gate!(@args $args);
        let call = wasm_component_middleware::Call::new(
            chain.next_id(),
            wasm_component_middleware::Direction::Import,
            $function,
        )
        .in_interface($interface, Some($version))
        .with_handles(&handles)
        .with_args(&arguments);
        chain.dispatch($gate.state, &call, |state| {
            let value = ($delegate)(state)?;
            let produced = gate!(@produced value $(, $produced)?);
            Ok((value, wasm_component_middleware::Completion { produced }))
        })
    }};
    (@produced $value:ident) => { Vec::<u32>::new() };
    (@produced $value:ident, $produced:expr) => { ($produced)(&$value) };
    (@args ()) => { wasm_component_middleware::Arguments::new() };
    (@args [$($name:ident = $value:expr),* $(,)?]) => {
        wasm_component_middleware::Arguments::new()
            $(.with_debug(stringify!($name), &$value))*
    };
}

pub(crate) use gate;
