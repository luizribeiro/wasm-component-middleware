use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use wasmtime::StoreContextMut;
use wasmtime::component::HasData;

use crate::{Call, Completion, Direction, MiddlewareView};

/// The [`HasData`] marker used with a generated `add_to_linker` function.
///
/// Generated host traits are implemented for [`Routing<'_, S>`], then linked
/// with `Routed<S>` so every method can enter the middleware chain. Use
/// [`route_imports!`](crate::route_imports) to generate that implementation.
pub struct Routed<S>(PhantomData<fn() -> S>);

impl<S> Routed<S> {
    /// Projects store data for a generated `add_to_linker` function.
    pub fn get(state: &mut S) -> Routing<'_, S> {
        Routing::new(state)
    }
}

impl<S: 'static> HasData for Routed<S> {
    type Data<'a> = Routing<'a, S>;
}

/// A temporary generated-host-trait receiver that routes into store data.
///
/// [`route_imports!`](crate::route_imports) implements generated host traits
/// for this receiver.
pub struct Routing<'a, S> {
    state: &'a mut S,
}

impl<'a, S> Routing<'a, S> {
    /// Projects a routing receiver from an application's store data.
    pub fn new(state: &'a mut S) -> Self {
        Self { state }
    }
}

impl<S: MiddlewareView + 'static> Routing<'_, S> {
    /// Dispatches one generated host-trait method through the store's chain.
    ///
    /// # Errors
    ///
    /// Returns a middleware refusal or the host implementation's error.
    pub fn dispatch<R>(
        &mut self,
        interface: &'static str,
        function: &str,
        args: &(dyn Debug + Send + Sync),
        body: impl FnOnce(&mut S) -> wasmtime::Result<R>,
    ) -> wasmtime::Result<R> {
        dispatch_import(self.state, Some(interface), function, args, body)
    }
}

/// Routes a host import closure through the chain in `state`.
///
/// Use this from a hand-written `func_wrap` closure when generated host traits
/// are not involved.
///
/// # Errors
///
/// Returns a middleware refusal or the host handler's error.
pub fn dispatch_import<S, R>(
    state: &mut S,
    interface: Option<&str>,
    function: &str,
    args: &(dyn Debug + Send + Sync),
    body: impl FnOnce(&mut S) -> wasmtime::Result<R>,
) -> wasmtime::Result<R>
where
    S: MiddlewareView + 'static,
{
    let chain = Arc::clone(state.middleware().chain());
    let call = Call {
        id: chain.next_id(),
        direction: Direction::Import,
        interface,
        function,
        handles: &[],
        args,
    };
    chain.dispatch(state, &call, |state| {
        body(state).map(|value| (value, Completion::default()))
    })
}

/// Routes one generated component export call through the store's chain.
///
/// # Errors
///
/// Returns a middleware refusal or the generated export call's error.
pub fn route_export<S, R>(
    mut store: StoreContextMut<'_, S>,
    interface: Option<&str>,
    function: &str,
    args: &(dyn Debug + Send + Sync),
    body: impl FnOnce(StoreContextMut<'_, S>) -> wasmtime::Result<R>,
) -> wasmtime::Result<R>
where
    S: MiddlewareView + 'static,
{
    let chain = Arc::clone(store.data_mut().middleware().chain());
    let call = Call {
        id: chain.next_id(),
        direction: Direction::Export,
        interface,
        function,
        handles: &[],
        args,
    };
    chain.dispatch_export(store, &call, body)
}

/// Implements a generated host trait by routing every method into a chain.
///
/// The generated bindings must opt into trappable imports with
/// `imports: { default: trappable }`. This lets a middleware refusal become a
/// Wasmtime error; for a function whose WIT result has no error case, the
/// refusal is observed by the guest as a trap. The application's ordinary
/// implementation of the same host trait remains the body of each call.
#[macro_export]
macro_rules! route_imports {
    (
        $host:path => $state:ty as $interface:literal {
            $(
                fn $method:ident(
                    &mut self
                    $(, $argument:ident: $argument_type:ty)*
                ) -> $return:ty;
            )*
        }
    ) => {
        impl $host for $crate::Routing<'_, $state> {
            $(
                fn $method(
                    &mut self,
                    $($argument: $argument_type),*
                ) -> $return {
                    let arguments = ($($argument.clone()),*);
                    let function = stringify!($method).replace('_', "-");
                    self.dispatch($interface, &function, &arguments, |state| {
                        <$state as $host>::$method(state $(, $argument)*)
                    })
                }
            )*
        }
    };
}

/// Routes one generated host-trait method through a [`Routing`](crate::Routing) receiver.
#[macro_export]
macro_rules! route_import {
    ($routing:expr, $interface:expr, $function:expr, $args:expr, $body:expr) => {{
        let arguments = $args;
        $routing.dispatch($interface, $function, &arguments, $body)
    }};
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use wasmtime::{AsContextMut, Engine, Store};

    use crate::{
        Chain, InvocationContext, MiddlewareCtx, MiddlewareView, Routing, dispatch_import,
        route_export,
    };

    struct State {
        middleware: Option<MiddlewareCtx<Self>>,
        calls: usize,
    }

    impl State {
        fn new() -> Self {
            Self {
                middleware: Some(MiddlewareCtx::new(
                    Arc::new(Chain::builder().build()),
                    InvocationContext::new("test"),
                )),
                calls: 0,
            }
        }
    }

    impl MiddlewareView for State {
        fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
            self.middleware.as_mut().unwrap()
        }
    }

    trait TestHost {
        fn generated_call(&mut self, increment: usize) -> wasmtime::Result<usize>;
    }

    impl TestHost for State {
        fn generated_call(&mut self, increment: usize) -> wasmtime::Result<usize> {
            self.calls += increment;
            Ok(self.calls)
        }
    }

    crate::route_imports! {
        TestHost => State as "example:test/host" {
            fn generated_call(&mut self, increment: usize) -> wasmtime::Result<usize>;
        }
    }

    #[test]
    fn routes_generated_and_plain_import_shapes() {
        let mut state = State::new();
        let first = dispatch_import(&mut state, None, "plain", &(), |state| {
            state.calls += 1;
            Ok(state.calls)
        })
        .unwrap();
        let second = TestHost::generated_call(&mut Routing::new(&mut state), 1).unwrap();

        assert_eq!((first, second), (1, 2));
    }

    #[test]
    fn routes_export_in_one_expression() {
        let engine = Engine::default();
        let mut store = Store::new(&engine, State::new());

        let value = route_export(store.as_context_mut(), None, "greet", &"Hello", |_store| {
            Ok("Hello, Ada!")
        })
        .unwrap();

        assert_eq!(value, "Hello, Ada!");
    }
}
