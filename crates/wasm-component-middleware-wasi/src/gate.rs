use std::marker::PhantomData;

use wasmtime::component::{HasData, Resource};

pub(crate) struct Gate<'a, T> {
    pub(crate) state: &'a mut T,
}

pub(crate) fn project<T>(state: &mut T) -> Gate<'_, T> {
    Gate { state }
}

pub(crate) struct Unrelayed;

pub(crate) struct GateData<T, M = Unrelayed>(PhantomData<fn() -> (T, M)>);

impl<T: 'static, M: 'static> HasData for GateData<T, M> {
    type Data<'a> = Gate<'a, T>;
}

pub(crate) fn produced_resource<T>(resource: &Resource<T>) -> Vec<u32>
where
    T: 'static,
{
    vec![resource.rep()]
}

pub(crate) fn produced_directories<T>(directories: &[(Resource<T>, String)]) -> Vec<u32>
where
    T: 'static,
{
    directories
        .iter()
        .map(|(descriptor, _)| descriptor.rep())
        .collect()
}

macro_rules! gate {
    (
        filesystem $gate:ident, $version:expr, $interface:literal, $function:literal,
        handles = [$($handle:expr),* $(,)?], args = $args:tt,
        delegate = $delegate:expr, denied = $denied:expr, wrapper = $wrapper:ty
        $(, produced = $produced:expr)?
    ) => {{
        let result = gate!(
            trap $gate, $version, $interface, $function,
            handles = [$($handle),*], args = $args,
            delegate = $delegate $(, produced = $produced)?
        );
        result.map_err(|error| match error.downcast::<wasm_component_middleware::Denied>() {
            Ok(_) => $denied,
            Err(error) => match error.downcast::<$wrapper>() {
                Ok(error) => error,
                Err(error) => <$wrapper>::trap(error),
            },
        })
    }};
    (
        filesystem_async $store:ident, $interface:literal, $function:literal,
        handles = $handles:expr, args = $args:tt,
        delegate = $delegate:expr, denied = $denied:expr, wrapper = $wrapper:ty
        $(, produced = $produced:expr)?
    ) => {{
        let result = gate!(
            async $store, $interface, $function,
            handles = $handles, args = $args,
            delegate = $delegate $(, produced = $produced)?
        );
        result.map_err(|error| match error.downcast::<wasm_component_middleware::Denied>() {
            Ok(_) => $denied,
            Err(error) => match error.downcast::<$wrapper>() {
                Ok(error) => error,
                Err(error) => <$wrapper>::trap(error),
            },
        })
    }};
    (
        trap $gate:ident, $version:expr, $interface:literal, $function:literal,
        handles = [$($handle:expr),* $(,)?], args = $args:tt,
        delegate = $delegate:expr $(, produced = $produced:expr)?
    ) => {{
        gate!(
            @sync $gate, $interface, $version, $function,
            handles = [$($handle.rep()),*], args = $args,
            delegate = $delegate,
            produced = gate!(@producer $(, $produced)?)
        )
    }};
    (
        trap_each $gate:ident, $version:expr, $interface:literal, $function:literal,
        handles = $resources:expr, args = $args:tt,
        delegate = $delegate:expr
    ) => {{
        gate!(
            @sync $gate, $interface, $version, $function,
            handles = $resources.iter().map(wasmtime::component::Resource::rep), args = $args,
            delegate = $delegate, produced = |_| Vec::<u32>::new()
        )
    }};
    (
        @sync $gate:ident, $interface:literal, $version:expr, $function:literal,
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
        .in_interface($interface, Some($version))
        .with_handles(&handles)
        .with_args(&arguments);
        chain.dispatch($gate.state, &call, |state| {
            let value = ($delegate)(state)?;
            let produced = ($produced)(&value);
            Ok((value, wasm_component_middleware::Completion { produced }))
        })
    }};
    (
        stream $gate:ident, $version:expr, $interface:literal, $function:literal,
        handles = [$($handle:expr),* $(,)?], args = $args:tt,
        delegate = $delegate:expr
    ) => {{
        let result = gate!(
            trap $gate, $version, $interface, $function,
            handles = [$($handle),*], args = $args,
            delegate = |state| Ok::<_, wasmtime::Error>(($delegate)(state))
        );
        result.unwrap_or_else(|error| match error.downcast::<wasm_component_middleware::Denied>() {
            Ok(denied) => Err(wasmtime_wasi::p2::StreamError::LastOperationFailed(denied.into())),
            Err(error) => Err(wasmtime_wasi::p2::StreamError::Trap(error)),
        })
    }};
    (
        access $store:ident, $interface:literal, $function:literal,
        handles = $handles:expr, args = $args:tt,
        delegate = $delegate:expr $(, produced = $produced:expr)?
    ) => {{
        let chain = std::sync::Arc::clone($store.data_mut().middleware().chain());
        let handles: Vec<u32> = ($handles).into_iter().collect();
        let arguments = gate!(@args $args);
        let call = wasm_component_middleware::Call::new(
            chain.next_id(),
            wasm_component_middleware::Direction::Import,
            $function,
        )
        .in_interface($interface, Some($crate::p3::WASI_VERSION))
        .with_handles(&handles)
        .with_args(&arguments);
        chain.dispatch_access($store, &call, |store| {
            let value = ($delegate)(store)?;
            let produced = gate!(@produced value $(, $produced)?);
            Ok((value, wasm_component_middleware::Completion { produced }))
        })
    }};
    (
        async $store:ident, $interface:literal, $function:literal,
        handles = $handles:expr, args = $args:tt,
        delegate = $delegate:expr $(, produced = $produced:expr)?
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
        .in_interface($interface, Some($crate::p3::WASI_VERSION))
        .with_handles(&handles)
        .with_args(&arguments);
        chain.dispatch_async($store, &call, || async {
            let value = ($delegate).await?;
            let produced = gate!(@produced value $(, $produced)?);
            Ok((value, wasm_component_middleware::Completion { produced }))
        }).await
    }};
    (@produced $value:ident) => {
        Vec::<u32>::new()
    };
    (@produced $value:ident, $produced:expr) => {
        ($produced)(&$value)
    };
    (@producer) => {
        |_| Vec::<u32>::new()
    };
    (@producer, $produced:expr) => {
        $produced
    };
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

pub(crate) use gate;
