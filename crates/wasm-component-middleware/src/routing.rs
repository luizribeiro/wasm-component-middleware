use std::marker::PhantomData;
use std::sync::Arc;

use wasmtime::StoreContextMut;
use wasmtime::component::HasData;

use crate::{Arguments, Call, Completion, Direction, MiddlewareView};

/// Identifies an interface implemented through [`route_imports!`](crate::route_imports).
///
/// Pass values emitted by the macro to [`verify_routing`](crate::verify_routing)
/// before instantiating a component.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RoutedInterface(&'static str);

impl RoutedInterface {
    /// Creates the marker emitted by [`route_imports!`](crate::route_imports).
    #[doc(hidden)]
    #[must_use]
    pub const fn from_static(name: &'static str) -> Self {
        Self(name)
    }

    /// Returns the canonical WIT interface name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        self.0
    }
}

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
        args: &Arguments,
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
    args: &Arguments,
    body: impl FnOnce(&mut S) -> wasmtime::Result<R>,
) -> wasmtime::Result<R>
where
    S: MiddlewareView + 'static,
{
    let chain = Arc::clone(state.middleware().chain());
    let mut call = Call::new(chain.next_id(), Direction::Import, function).with_args(args);
    if let Some(interface) = interface {
        call = call.in_interface(interface, None);
    }
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
    args: &Arguments,
    body: impl FnOnce(StoreContextMut<'_, S>) -> wasmtime::Result<R>,
) -> wasmtime::Result<R>
where
    S: MiddlewareView + 'static,
{
    let chain = Arc::clone(store.data_mut().middleware().chain());
    let mut call = Call::new(chain.next_id(), Direction::Export, function).with_args(args);
    if let Some(interface) = interface {
        call = call.in_interface(interface, None);
    }
    chain.dispatch_export(store, &call, body)
}

/// Implements a generated host trait by routing every method into a chain.
///
/// The generated bindings must opt into trappable imports with
/// `imports: { default: trappable }`. This lets a middleware refusal become a
/// Wasmtime error; for a function whose WIT result has no error case, the
/// refusal is observed by the guest as a trap. The application's ordinary
/// implementation of the same host trait remains the body of each call.
/// The named constant can be passed to [`verify_routing`](crate::verify_routing)
/// without repeating the interface string.
#[macro_export]
macro_rules! route_imports {
    (
        $visibility:vis const $route:ident: $host:path => $state:ty as $interface:literal {
            $(
                fn $method:ident(
                    &mut self
                    $(, $argument:ident: $argument_type:ty)*
                ) -> $return:ty;
            )*
        }
    ) => {
        $visibility const $route: $crate::RoutedInterface =
            $crate::RoutedInterface::from_static($interface);

        impl $host for $crate::Routing<'_, $state> {
            $(
                fn $method(
                    &mut self,
                    $($argument: $argument_type),*
                ) -> $return {
                    let arguments = $crate::Arguments::new()
                        $(.with_debug(stringify!($argument), &$argument))*;
                    let function = stringify!($method).replace('_', "-");
                    self.dispatch($interface, &function, &arguments, |state| {
                        <$state as $host>::$method(state $(, $argument)*)
                    })
                }
            )*
        }
    };
}

#[cfg(test)]
mod tests {
    use wasmtime::{AsContextMut, Engine, Store};

    use crate::{
        Arguments, Chain, InvocationContext, MiddlewareCtx, MiddlewareView, Routing,
        dispatch_import, route_export,
    };

    struct State {
        middleware: Option<MiddlewareCtx<Self>>,
        calls: usize,
    }

    impl State {
        fn new() -> Self {
            Self {
                middleware: Some(MiddlewareCtx::new(
                    Chain::builder().build(),
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
        const TEST_HOST: TestHost => State as "example:test/host" {
            fn generated_call(&mut self, increment: usize) -> wasmtime::Result<usize>;
        }
    }

    #[test]
    fn routes_generated_and_plain_import_shapes() {
        let mut state = State::new();
        let first = dispatch_import(&mut state, None, "plain", &Arguments::new(), |state| {
            state.calls += 1;
            Ok(state.calls)
        })
        .unwrap();
        let second = TestHost::generated_call(&mut Routing::new(&mut state), 1).unwrap();

        assert_eq!((first, second), (1, 2));
        assert_eq!(TEST_HOST.name(), "example:test/host");
    }

    #[test]
    fn routes_export_in_one_expression() {
        let engine = Engine::default();
        let mut store = Store::new(&engine, State::new());

        let args = Arguments::new().with("greeting", "Hello");
        let value = route_export(store.as_context_mut(), None, "greet", &args, |_store| {
            Ok("Hello, Ada!")
        })
        .unwrap();

        assert_eq!(value, "Hello, Ada!");
    }
}
