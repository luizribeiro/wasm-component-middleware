use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::Chain;

type Extensions = HashMap<TypeId, Box<dyn Any + Send + Sync>>;

static NEXT_INVOCATION_ID: AtomicU64 = AtomicU64::new(1);

/// Identifies a plugin invocation and carries policy-specific state.
pub struct InvocationContext {
    id: u64,
    plugin: String,
    extensions: Extensions,
}

impl InvocationContext {
    /// Creates an empty context for `plugin`.
    pub fn new(plugin: impl Into<String>) -> Self {
        Self {
            id: NEXT_INVOCATION_ID.fetch_add(1, Ordering::Relaxed),
            plugin: plugin.into(),
            extensions: HashMap::new(),
        }
    }

    /// Returns the plugin name used by middleware diagnostics and policy.
    #[must_use]
    pub fn plugin(&self) -> &str {
        &self.plugin
    }

    pub(crate) fn id(&self) -> u64 {
        self.id
    }

    /// Inserts type-keyed policy state, returning the previous value if any.
    pub fn insert<T: Send + Sync + 'static>(&mut self, value: T) -> Option<T> {
        self.extensions
            .insert(TypeId::of::<T>(), Box::new(value))
            .and_then(|old| old.downcast().ok())
            .map(|old| *old)
    }

    /// Returns shared access to type-keyed policy state.
    #[must_use]
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.extensions
            .get(&TypeId::of::<T>())
            .and_then(|value| value.downcast_ref())
    }

    /// Returns mutable access to type-keyed policy state.
    pub fn get_mut<T: Send + Sync + 'static>(&mut self) -> Option<&mut T> {
        self.extensions
            .get_mut(&TypeId::of::<T>())
            .and_then(|value| value.downcast_mut())
    }
}

/// Middleware state embedded in an application's Wasmtime store data.
pub struct MiddlewareCtx<S> {
    chain: Arc<Chain<S>>,
    context: InvocationContext,
}

impl<S> MiddlewareCtx<S> {
    /// Combines a per-instance chain with invocation metadata.
    pub fn new(chain: Arc<Chain<S>>, context: InvocationContext) -> Self {
        Self { chain, context }
    }

    /// Returns the chain used to route calls for this store.
    #[must_use]
    pub fn chain(&self) -> &Arc<Chain<S>> {
        &self.chain
    }

    /// Returns the invocation metadata and policy state.
    #[must_use]
    pub fn context(&self) -> &InvocationContext {
        &self.context
    }

    /// Returns mutable invocation metadata and policy state.
    pub fn context_mut(&mut self) -> &mut InvocationContext {
        &mut self.context
    }
}

/// Projects middleware state out of an application's Wasmtime store data.
pub trait MiddlewareView: Sized + Send {
    /// Returns the chain and invocation context for this store.
    fn middleware(&mut self) -> &mut MiddlewareCtx<Self>;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{Chain, InvocationContext, MiddlewareCtx};

    #[test]
    fn invocation_extensions_are_keyed_by_type() {
        let mut context = InvocationContext::new("greeter");

        assert_eq!(context.insert(4_u32), None);
        assert_eq!(context.insert(String::from("blue")), None);
        *context.get_mut::<u32>().unwrap() += 1;

        assert_eq!(context.plugin(), "greeter");
        assert_eq!(context.get::<u32>(), Some(&5));
        assert_eq!(context.get::<String>().map(String::as_str), Some("blue"));
    }

    #[test]
    fn middleware_context_retains_chain_and_invocation() {
        let chain = Chain::<State>::builder().build();
        let context = MiddlewareCtx::new(Arc::clone(&chain), InvocationContext::new("greeter"));

        assert!(Arc::ptr_eq(context.chain(), &chain));
        assert_eq!(context.context().plugin(), "greeter");
    }

    struct State;
}
