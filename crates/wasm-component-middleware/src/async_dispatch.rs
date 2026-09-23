use std::future::Future;

use wasmtime::component::{Access, Accessor, HasData};

use crate::chain::{ActiveLayers, CancellationQueue, PendingCancellation, outcome};
use crate::types::OwnedCall;
use crate::{Call, Chain, Completion, Denied, MiddlewareView, Outcome};

/// Lends mutable store data to a synchronous closure.
///
/// This is the narrow capability needed to bracket an asynchronous body
/// without holding a mutable borrow across an await point.
pub trait StateAccess<S> {
    /// Runs `operation` with temporary mutable access to the store data.
    fn with_state<R>(&mut self, operation: impl FnOnce(&mut S) -> R) -> R;
}

impl<T, D> StateAccess<T> for &Accessor<T, D>
where
    T: 'static,
    D: HasData + ?Sized,
{
    fn with_state<R>(&mut self, operation: impl FnOnce(&mut T) -> R) -> R {
        Accessor::with(self, |mut access| operation(access.data_mut()))
    }
}

impl<T, D> StateAccess<T> for Access<'_, T, D>
where
    T: 'static,
    D: HasData + ?Sized,
{
    fn with_state<R>(&mut self, operation: impl FnOnce(&mut T) -> R) -> R {
        operation(self.data_mut())
    }
}

impl<S> StateAccess<S> for &mut S {
    fn with_state<R>(&mut self, operation: impl FnOnce(&mut S) -> R) -> R {
        operation(self)
    }
}

impl<S: 'static> Chain<S> {
    /// Runs an asynchronous body with mutable access to an access wrapper.
    ///
    /// This combines [`Chain::dispatch_access`] with the cancellation behavior
    /// of [`Chain::dispatch_async`] for host methods that take [`Access`] by
    /// value and remain asynchronous.
    ///
    /// # Errors
    ///
    /// Returns a refusal from a layer or an error from `body`.
    pub async fn dispatch_access_async<A, P, R>(
        &self,
        mut access: A,
        call: &Call<'_>,
        parameters: P,
        body: impl AsyncFnOnce(&mut A, P) -> wasmtime::Result<(R, Completion)>,
    ) -> wasmtime::Result<R>
    where
        A: StateAccess<S>,
        S: MiddlewareView,
    {
        let (guard, denial) = self.start_async_dispatch(&mut access, call);
        let result = match denial {
            Some(denied) => Err(denied.into()),
            None => body(&mut access, parameters).await,
        };
        Self::finish_async_dispatch(guard, &mut access, call, result)
    }

    /// Runs a borrowed-state asynchronous body between middleware hooks.
    ///
    /// Use this when a host trait lends `&mut S` to an async method instead of
    /// exposing a Wasmtime [`Accessor`]. The WIT functions that require this
    /// helper are not declared `async`; dropping the returned future therefore
    /// covers host-side abandonment and store teardown, not guest subtask
    /// cancellation.
    ///
    /// # Errors
    ///
    /// Returns a refusal from a layer or an error from `body`.
    pub async fn dispatch_state_async<R>(
        &self,
        state: &mut S,
        call: &Call<'_>,
        body: impl AsyncFnOnce(&mut S) -> wasmtime::Result<(R, Completion)>,
    ) -> wasmtime::Result<R>
    where
        S: MiddlewareView,
    {
        let mut access = state;
        let (guard, denial) = self.start_async_dispatch(&mut access, call);
        let result = match denial {
            Some(denied) => Err(denied.into()),
            None => body(access).await,
        };
        Self::finish_async_dispatch(guard, &mut access, call, result)
    }

    /// Runs a synchronous store-aware body between middleware hooks.
    ///
    /// # Errors
    ///
    /// Returns a refusal from a layer or an error from `body`.
    pub fn dispatch_access<A, R>(
        &self,
        mut access: A,
        call: &Call<'_>,
        body: impl FnOnce(&mut A) -> wasmtime::Result<(R, Completion)>,
    ) -> wasmtime::Result<R>
    where
        A: StateAccess<S>,
        S: MiddlewareView,
    {
        let (frames, denial) = access.with_state(|state| {
            Self::deliver_cancellations(state);
            self.before(state, call)
        });
        let result = match denial {
            Some(denied) => Err(denied.into()),
            None => body(&mut access),
        };
        let outcome = outcome(result.as_ref().map(|(_, completion)| completion));
        access.with_state(|state| Self::after(state, call, frames, outcome));
        result.map(|(value, _)| value)
    }

    /// Runs an asynchronous body between synchronous middleware hooks.
    ///
    /// Dropping the returned future after its body starts queues the active
    /// layers' `after` hooks with [`Outcome::Cancelled`]. The hooks run once
    /// the store is next available, at the start of its next dispatch or when
    /// an export dispatch returns. They do not run if the store is dropped
    /// first.
    ///
    /// # Errors
    ///
    /// Returns a refusal from a layer or an error from `body`.
    pub async fn dispatch_async<A, R, F, Fut>(
        &self,
        mut access: A,
        call: &Call<'_>,
        body: F,
    ) -> wasmtime::Result<R>
    where
        A: StateAccess<S>,
        S: MiddlewareView,
        F: FnOnce() -> Fut,
        Fut: Future<Output = wasmtime::Result<(R, Completion)>>,
    {
        let (guard, denial) = self.start_async_dispatch(&mut access, call);
        let result = match denial {
            Some(denied) => Err(denied.into()),
            None => body().await,
        };
        Self::finish_async_dispatch(guard, &mut access, call, result)
    }

    fn start_async_dispatch<A>(
        &self,
        access: &mut A,
        call: &Call<'_>,
    ) -> (CancellationGuard<S>, Option<Denied>)
    where
        A: StateAccess<S>,
        S: MiddlewareView,
    {
        let (cancellations, frames, denial) = access.with_state(|state| {
            Self::deliver_cancellations(state);
            let cancellations = state.middleware().cancellations();
            let (frames, denial) = self.before(state, call);
            (cancellations, frames, denial)
        });
        (
            CancellationGuard {
                cancellations,
                call: Some(OwnedCall::from_call(call)),
                frames: Some(frames),
            },
            denial,
        )
    }

    fn finish_async_dispatch<A, R>(
        mut guard: CancellationGuard<S>,
        access: &mut A,
        call: &Call<'_>,
        result: wasmtime::Result<(R, Completion)>,
    ) -> wasmtime::Result<R>
    where
        A: StateAccess<S>,
    {
        let outcome = outcome(result.as_ref().map(|(_, completion)| completion));
        guard.finish(access, call, outcome);
        result.map(|(value, _)| value)
    }
}

struct CancellationGuard<S> {
    cancellations: CancellationQueue<S>,
    call: Option<OwnedCall>,
    frames: Option<ActiveLayers<S>>,
}

impl<S: 'static> CancellationGuard<S> {
    fn finish<A>(&mut self, access: &mut A, call: &Call<'_>, outcome: Outcome<'_>)
    where
        A: StateAccess<S>,
    {
        if let Some(frames) = self.frames.take() {
            access.with_state(|state| Chain::after(state, call, frames, outcome));
        }
    }
}

impl<S> Drop for CancellationGuard<S> {
    fn drop(&mut self) {
        if let (Some(frames), Some(call)) = (self.frames.take(), self.call.take()) {
            self.cancellations
                .push(PendingCancellation { call, frames });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::future::{Future, pending};
    use std::sync::{Arc, Mutex};
    use std::task::{Context, Poll, Waker};

    use crate::{
        Call, Chain, Completion, Denied, Direction, InvocationContext, Layer, MiddlewareCtx,
        MiddlewareView, Outcome, StateAccess,
    };

    struct State {
        middleware: Option<MiddlewareCtx<Self>>,
        events: Vec<String>,
        deny: bool,
    }

    impl State {
        fn new(chain: &Arc<Chain<Self>>) -> Self {
            Self {
                middleware: Some(MiddlewareCtx::new(
                    Arc::clone(chain),
                    InvocationContext::new("test"),
                )),
                events: Vec::new(),
                deny: false,
            }
        }
    }

    impl MiddlewareView for State {
        fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
            self.middleware.as_mut().unwrap()
        }
    }

    struct MemoryAccess(RefCell<State>);

    impl StateAccess<State> for &MemoryAccess {
        fn with_state<R>(&mut self, operation: impl FnOnce(&mut State) -> R) -> R {
            operation(&mut self.0.borrow_mut())
        }
    }

    struct SendAccess(Mutex<State>);

    impl StateAccess<State> for &SendAccess {
        fn with_state<R>(&mut self, operation: impl FnOnce(&mut State) -> R) -> R {
            operation(&mut self.0.lock().unwrap())
        }
    }

    struct Observer(&'static str);

    impl Layer<State> for Observer {
        type Frame = &'static str;

        fn before(&self, state: &mut State, _: &Call<'_>) -> Result<Self::Frame, Denied> {
            state.events.push(format!("before {}", self.0));
            if state.deny {
                Err(Denied::new("blocked"))
            } else {
                Ok(self.0)
            }
        }

        fn after(&self, state: &mut State, _: &Call<'_>, frame: Self::Frame, outcome: Outcome<'_>) {
            assert_eq!(frame, self.0);
            let event = match outcome {
                Outcome::Returned(completion) => format!("returned {:?}", completion.produced),
                Outcome::Failed(error) if error.downcast_ref::<Denied>().is_some() => {
                    "denied".into()
                }
                Outcome::Failed(_) => "failed".into(),
                Outcome::Cancelled => "cancelled".into(),
            };
            state.events.push(format!("after {} {event}", self.0));
        }
    }

    fn call() -> Call<'static> {
        Call::new(1, Direction::Import, "work")
    }

    fn run_ready<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => output,
            Poll::Pending => unreachable!("test future should be ready"),
        }
    }

    async fn record_access_body(
        access: &mut &MemoryAccess,
        value: i32,
    ) -> wasmtime::Result<(i32, Completion)> {
        access.with_state(|state| state.events.push("body".into()));
        Ok((value, Completion { produced: vec![9] }))
    }

    async fn record_send_access_body(
        access: &mut &SendAccess,
        value: i32,
    ) -> wasmtime::Result<(i32, Completion)> {
        access.with_state(|state| state.events.push("body".into()));
        Ok((value, Completion::default()))
    }

    #[test]
    fn dispatches_a_borrowed_state_future() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let mut state = State::new(&chain);

        let value = run_ready(chain.dispatch_state_async(
            &mut state,
            &call(),
            async |state: &mut State| {
                state.events.push("body".into());
                Ok((17, Completion::default()))
            },
        ))
        .unwrap();

        assert_eq!(value, 17);
        assert_eq!(
            state.events,
            ["before observer", "body", "after observer returned []"]
        );
    }

    #[test]
    fn dispatches_an_access_future() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let access = MemoryAccess(RefCell::new(State::new(&chain)));

        let value =
            run_ready(chain.dispatch_access_async(&access, &call(), 17, record_access_body))
                .unwrap();

        assert_eq!(value, 17);
        assert_eq!(
            access.0.borrow().events,
            ["before observer", "body", "after observer returned [9]"]
        );
    }

    #[test]
    fn borrowed_state_future_reports_body_failure() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let mut state = State::new(&chain);

        let error = run_ready(chain.dispatch_state_async(
            &mut state,
            &call(),
            async |_state: &mut State| {
                Err::<((), Completion), _>(wasmtime::Error::msg("body failed"))
            },
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "body failed");
        assert_eq!(state.events, ["before observer", "after observer failed"]);
    }

    #[test]
    fn borrowed_state_future_reports_denial_without_running_body() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let mut state = State::new(&chain);
        state.deny = true;
        let body_ran = Cell::new(false);

        let error = run_ready(chain.dispatch_state_async(
            &mut state,
            &call(),
            async |_state: &mut State| {
                body_ran.set(true);
                Ok(((), Completion::default()))
            },
        ))
        .unwrap_err();

        assert_eq!(error.downcast_ref::<Denied>().unwrap().reason(), "blocked");
        assert!(!body_ran.get());
        assert_eq!(state.events, ["before observer"]);
    }

    #[test]
    fn access_future_reports_body_failure() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let access = MemoryAccess(RefCell::new(State::new(&chain)));

        let error = run_ready(chain.dispatch_access_async(
            &access,
            &call(),
            (),
            async |_access: &mut &MemoryAccess, (): ()| {
                Err::<((), Completion), _>(wasmtime::Error::msg("body failed"))
            },
        ))
        .unwrap_err();

        assert_eq!(error.to_string(), "body failed");
        assert_eq!(
            access.0.borrow().events,
            ["before observer", "after observer failed"]
        );
    }

    #[test]
    fn access_future_reports_denial_without_running_body() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let mut state = State::new(&chain);
        state.deny = true;
        let access = MemoryAccess(RefCell::new(state));
        let body_ran = Cell::new(false);

        let error = run_ready(chain.dispatch_access_async(
            &access,
            &call(),
            (),
            async |_access: &mut &MemoryAccess, (): ()| {
                body_ran.set(true);
                Ok(((), Completion::default()))
            },
        ))
        .unwrap_err();

        assert_eq!(error.downcast_ref::<Denied>().unwrap().reason(), "blocked");
        assert!(!body_ran.get());
        assert_eq!(access.0.borrow().events, ["before observer"]);
    }

    #[test]
    fn cancelled_borrowed_state_future_is_delivered_before_the_next_call() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let mut state = State::new(&chain);
        let call = call();
        let mut future = Box::pin(chain.dispatch_state_async(
            &mut state,
            &call,
            async |_state: &mut State| pending::<wasmtime::Result<((), Completion)>>().await,
        ));
        let mut context = Context::from_waker(Waker::noop());

        assert!(future.as_mut().poll(&mut context).is_pending());
        drop(future);
        assert_eq!(state.events, ["before observer"]);

        run_ready(chain.dispatch_state_async(
            &mut state,
            &Call::new(2, Direction::Import, "next"),
            async |_state: &mut State| Ok(((), Completion::default())),
        ))
        .unwrap();

        assert_eq!(
            state.events,
            [
                "before observer",
                "after observer cancelled",
                "before observer",
                "after observer returned []",
            ]
        );
    }

    #[test]
    fn cancelled_access_future_is_delivered_before_the_next_call() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let access = MemoryAccess(RefCell::new(State::new(&chain)));
        let call = call();
        let mut future = Box::pin(chain.dispatch_access_async(
            &access,
            &call,
            (),
            async |_access: &mut &MemoryAccess, (): ()| {
                pending::<wasmtime::Result<((), Completion)>>().await
            },
        ));
        let mut context = Context::from_waker(Waker::noop());

        assert!(future.as_mut().poll(&mut context).is_pending());
        drop(future);
        assert_eq!(access.0.borrow().events, ["before observer"]);

        run_ready(chain.dispatch_access_async(
            &access,
            &Call::new(2, Direction::Import, "next"),
            (),
            async |_access: &mut &MemoryAccess, (): ()| Ok(((), Completion::default())),
        ))
        .unwrap();

        assert_eq!(
            access.0.borrow().events,
            [
                "before observer",
                "after observer cancelled",
                "before observer",
                "after observer returned []",
            ]
        );
    }

    #[test]
    fn reports_returned_completion() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let access = MemoryAccess(RefCell::new(State::new(&chain)));

        let value = run_ready(chain.dispatch_async(&access, &call(), || async {
            Ok((17, Completion { produced: vec![9] }))
        }))
        .unwrap();

        assert_eq!(value, 17);
        assert_eq!(
            access.0.borrow().events,
            ["before observer", "after observer returned [9]"]
        );
    }

    #[test]
    fn reports_body_failure() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let access = MemoryAccess(RefCell::new(State::new(&chain)));

        let error = run_ready(
            chain.dispatch_async::<_, (), _, _>(&access, &call(), || async {
                Err(wasmtime::Error::msg("body failed"))
            }),
        )
        .unwrap_err();

        assert_eq!(error.to_string(), "body failed");
        assert_eq!(
            access.0.borrow().events,
            ["before observer", "after observer failed"]
        );
    }

    #[test]
    fn reports_denial_without_running_body() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let mut state = State::new(&chain);
        state.deny = true;
        let access = MemoryAccess(RefCell::new(state));
        let body_ran = Cell::new(false);

        let error = run_ready(chain.dispatch_async(&access, &call(), || async {
            body_ran.set(true);
            Ok(((), Completion::default()))
        }))
        .unwrap_err();

        assert_eq!(error.downcast_ref::<Denied>().unwrap().reason(), "blocked");
        assert!(!body_ran.get());
        assert_eq!(access.0.borrow().events, ["before observer"]);
    }

    #[test]
    fn queued_cancellation_precedes_the_next_call() {
        let chain = Chain::builder()
            .layer(Observer("outer"))
            .layer(Observer("inner"))
            .build();
        let access = MemoryAccess(RefCell::new(State::new(&chain)));
        let started = Cell::new(false);
        let call = call();
        let mut future = Box::pin(
            chain.dispatch_async::<_, (), _, _>(&access, &call, || async {
                started.set(true);
                pending().await
            }),
        );
        let mut context = Context::from_waker(Waker::noop());

        assert!(future.as_mut().poll(&mut context).is_pending());
        assert!(started.get());
        drop(future);

        assert_eq!(access.0.borrow().events, ["before outer", "before inner"]);

        let next_call = Call::new(2, Direction::Import, "next");
        run_ready(chain.dispatch_async(&access, &next_call, || async {
            Ok(((), Completion::default()))
        }))
        .unwrap();

        assert_eq!(
            access.0.borrow().events,
            [
                "before outer",
                "before inner",
                "after inner cancelled",
                "after outer cancelled",
                "before outer",
                "before inner",
                "after inner returned []",
                "after outer returned []",
            ]
        );
    }

    #[test]
    fn dispatch_future_is_send_when_inputs_are_send() {
        fn assert_send(_: impl Send) {}

        let chain = Chain::builder().layer(Observer("observer")).build();
        let access = SendAccess(Mutex::new(State::new(&chain)));
        let call = call();

        assert_send(
            chain.dispatch_async(&access, &call, || async { Ok(((), Completion::default())) }),
        );
    }

    #[test]
    fn borrowed_state_dispatch_future_is_send_when_inputs_are_send() {
        fn assert_send(_: impl Send) {}

        let chain = Chain::builder().layer(Observer("observer")).build();
        let mut state = State::new(&chain);
        let call = call();

        assert_send(
            chain.dispatch_state_async(&mut state, &call, async |_state: &mut State| {
                Ok(((), Completion::default()))
            }),
        );
    }

    #[test]
    fn access_dispatch_future_is_send_when_inputs_are_send() {
        fn assert_send(_: impl Send) {}

        let chain = Chain::builder().layer(Observer("observer")).build();
        let access = SendAccess(Mutex::new(State::new(&chain)));
        let call = call();

        assert_send(chain.dispatch_access_async(&access, &call, 17, record_send_access_body));
    }
}
