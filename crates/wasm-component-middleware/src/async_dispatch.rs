use std::future::Future;

use crate::chain::{ActiveLayers, outcome};
use crate::{Call, Chain, Completion, Outcome};

/// Lends mutable store data to a synchronous closure.
///
/// This is the narrow capability needed to bracket an asynchronous body
/// without holding a mutable borrow across an await point.
pub trait StateAccess<S> {
    /// Runs `operation` with temporary mutable access to the store data.
    fn with_state<R>(&self, operation: impl FnOnce(&mut S) -> R) -> R;
}

impl<S: 'static> Chain<S> {
    /// Runs an asynchronous body between synchronous middleware hooks.
    ///
    /// Dropping the returned future after its body starts runs the active
    /// layers' `after` hooks with [`Outcome::Cancelled`].
    ///
    /// # Errors
    ///
    /// Returns a refusal from a layer or an error from `body`.
    pub async fn dispatch_async<A, R, F, Fut>(
        &self,
        access: &A,
        call: &Call<'_>,
        body: F,
    ) -> wasmtime::Result<R>
    where
        A: StateAccess<S>,
        F: FnOnce(&A) -> Fut,
        Fut: Future<Output = wasmtime::Result<(R, Completion)>>,
    {
        let (frames, denial) = access.with_state(|state| self.before(state, call));
        let mut guard = CancellationGuard {
            access,
            call,
            frames: Some(frames),
        };
        let result = match denial {
            Some(denied) => Err(denied.into()),
            None => body(access).await,
        };
        let outcome = outcome(result.as_ref().map(|(_, completion)| completion));
        guard.finish(outcome);
        result.map(|(value, _)| value)
    }
}

struct CancellationGuard<'access, 'call, 'data, S: 'static, A>
where
    A: StateAccess<S>,
{
    access: &'access A,
    call: &'call Call<'data>,
    frames: Option<ActiveLayers<S>>,
}

impl<S: 'static, A> CancellationGuard<'_, '_, '_, S, A>
where
    A: StateAccess<S>,
{
    fn finish(&mut self, outcome: Outcome<'_>) {
        if let Some(frames) = self.frames.take() {
            self.access
                .with_state(|state| Chain::after(state, self.call, frames, outcome));
        }
    }
}

impl<S: 'static, A> Drop for CancellationGuard<'_, '_, '_, S, A>
where
    A: StateAccess<S>,
{
    fn drop(&mut self) {
        self.finish(Outcome::Cancelled);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::future::{Future, pending};
    use std::sync::Mutex;
    use std::task::{Context, Poll, Waker};

    use crate::{Call, Chain, Completion, Denied, Direction, Layer, Outcome, StateAccess};

    #[derive(Default)]
    struct State {
        events: Vec<String>,
        deny: bool,
    }

    struct MemoryAccess(RefCell<State>);

    impl StateAccess<State> for MemoryAccess {
        fn with_state<R>(&self, operation: impl FnOnce(&mut State) -> R) -> R {
            operation(&mut self.0.borrow_mut())
        }
    }

    struct SendAccess(Mutex<State>);

    impl StateAccess<State> for SendAccess {
        fn with_state<R>(&self, operation: impl FnOnce(&mut State) -> R) -> R {
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

    #[test]
    fn reports_returned_completion() {
        let chain = Chain::builder().layer(Observer("observer")).build();
        let access = MemoryAccess(RefCell::new(State::default()));

        let value = run_ready(chain.dispatch_async(&access, &call(), |_| async {
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
        let access = MemoryAccess(RefCell::new(State::default()));

        let error = run_ready(
            chain.dispatch_async::<_, (), _, _>(&access, &call(), |_| async {
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
        let access = MemoryAccess(RefCell::new(State {
            deny: true,
            ..State::default()
        }));
        let body_ran = Cell::new(false);

        let error = run_ready(chain.dispatch_async(&access, &call(), |_| async {
            body_ran.set(true);
            Ok(((), Completion::default()))
        }))
        .unwrap_err();

        assert_eq!(error.downcast_ref::<Denied>().unwrap().reason(), "blocked");
        assert!(!body_ran.get());
        assert_eq!(access.0.borrow().events, ["before observer"]);
    }

    #[test]
    fn dropped_body_reports_cancellation() {
        let chain = Chain::builder()
            .layer(Observer("outer"))
            .layer(Observer("inner"))
            .build();
        let access = MemoryAccess(RefCell::new(State::default()));
        let started = Cell::new(false);
        let call = call();
        let mut future = Box::pin(
            chain.dispatch_async::<_, (), _, _>(&access, &call, |_| async {
                started.set(true);
                pending().await
            }),
        );
        let mut context = Context::from_waker(Waker::noop());

        assert!(future.as_mut().poll(&mut context).is_pending());
        assert!(started.get());
        drop(future);

        assert_eq!(
            access.0.borrow().events,
            [
                "before outer",
                "before inner",
                "after inner cancelled",
                "after outer cancelled",
            ]
        );
    }

    #[test]
    fn dispatch_future_is_send_when_inputs_are_send() {
        fn assert_send(_: impl Send) {}

        let chain = Chain::builder().layer(Observer("observer")).build();
        let access = SendAccess(Mutex::new(State::default()));
        let call = call();

        assert_send(chain.dispatch_async(&access, &call, |_| async {
            Ok(((), Completion::default()))
        }));
    }
}
