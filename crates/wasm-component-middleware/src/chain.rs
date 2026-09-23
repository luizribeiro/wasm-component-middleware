use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use wasmtime::{AsContextMut, StoreContextMut};

use crate::{Call, Completion, Denied, Layer, Outcome};

pub(crate) trait ActiveLayer<S>: Send {
    fn after(self: Box<Self>, state: &mut S, call: &Call<'_>, outcome: Outcome<'_>);
}

struct ActiveFrame<L: Layer<S>, S> {
    layer: Arc<L>,
    frame: L::Frame,
    state: std::marker::PhantomData<fn(&mut S)>,
}

impl<L, S> ActiveLayer<S> for ActiveFrame<L, S>
where
    L: Layer<S>,
{
    fn after(self: Box<Self>, state: &mut S, call: &Call<'_>, outcome: Outcome<'_>) {
        self.layer.after(state, call, self.frame, outcome);
    }
}

trait ErasedLayer<S>: Send + Sync {
    fn before(
        self: Arc<Self>,
        state: &mut S,
        call: &Call<'_>,
    ) -> Result<Box<dyn ActiveLayer<S>>, Denied>;
}

impl<L, S: 'static> ErasedLayer<S> for L
where
    L: Layer<S>,
{
    fn before(
        self: Arc<Self>,
        state: &mut S,
        call: &Call<'_>,
    ) -> Result<Box<dyn ActiveLayer<S>>, Denied> {
        let frame = Layer::before(self.as_ref(), state, call)?;
        Ok(Box::new(ActiveFrame {
            layer: self,
            frame,
            state: std::marker::PhantomData,
        }))
    }
}

pub(crate) type ActiveLayers<S> = Vec<Box<dyn ActiveLayer<S>>>;

/// An ordered, type-erased collection of middleware layers.
pub struct Chain<S> {
    layers: Vec<Arc<dyn ErasedLayer<S>>>,
    next_id: AtomicU64,
}

impl<S: 'static> Chain<S> {
    /// Starts building a chain for store data of type `S`.
    #[must_use]
    pub fn builder() -> ChainBuilder<S> {
        ChainBuilder { layers: Vec::new() }
    }

    /// Returns an identifier unique to this chain.
    #[must_use]
    pub fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Runs an import-like body between the chain's hooks.
    ///
    /// # Errors
    ///
    /// Returns a refusal from a layer or an error from `body`.
    pub fn dispatch<R>(
        &self,
        state: &mut S,
        call: &Call<'_>,
        body: impl FnOnce(&mut S) -> wasmtime::Result<(R, Completion)>,
    ) -> wasmtime::Result<R> {
        let (frames, denial) = self.before(state, call);
        let result = match denial {
            Some(denied) => Err(denied.into()),
            None => body(state),
        };
        let outcome = outcome(result.as_ref().map(|(_, completion)| completion));
        Self::after(state, call, frames, outcome);
        result.map(|(value, _)| value)
    }

    /// Runs an export-like body that needs mutable access to the store.
    ///
    /// # Errors
    ///
    /// Returns a refusal from a layer or an error from `body`.
    pub fn dispatch_export<R>(
        &self,
        mut store: StoreContextMut<'_, S>,
        call: &Call<'_>,
        body: impl FnOnce(StoreContextMut<'_, S>) -> wasmtime::Result<R>,
    ) -> wasmtime::Result<R> {
        let (frames, denial) = self.before(store.data_mut(), call);
        let result = match denial {
            Some(denied) => Err(denied.into()),
            None => body(store.as_context_mut()),
        };
        let completion = Completion::default();
        let outcome = outcome(result.as_ref().map(|_| &completion));
        Self::after(store.data_mut(), call, frames, outcome);
        result
    }

    pub(crate) fn before(
        &self,
        state: &mut S,
        call: &Call<'_>,
    ) -> (ActiveLayers<S>, Option<Denied>) {
        let mut frames = Vec::with_capacity(self.layers.len());
        for layer in &self.layers {
            match Arc::clone(layer).before(state, call) {
                Ok(frame) => frames.push(frame),
                Err(denied) => return (frames, Some(denied)),
            }
        }
        (frames, None)
    }

    pub(crate) fn after(
        state: &mut S,
        call: &Call<'_>,
        frames: ActiveLayers<S>,
        outcome: Outcome<'_>,
    ) {
        for frame in frames.into_iter().rev() {
            frame.after(state, call, outcome);
        }
    }
}

pub(crate) fn outcome<'a>(result: Result<&'a Completion, &'a wasmtime::Error>) -> Outcome<'a> {
    match result {
        Ok(completion) => Outcome::Returned(completion),
        Err(error) => Outcome::Failed(error),
    }
}

/// Builds a [`Chain`] while preserving layer registration order.
pub struct ChainBuilder<S> {
    layers: Vec<Arc<dyn ErasedLayer<S>>>,
}

impl<S: 'static> ChainBuilder<S> {
    /// Appends a layer to the inside of the chain.
    #[must_use]
    pub fn layer(mut self, layer: impl Layer<S>) -> Self {
        self.layers.push(Arc::new(layer));
        self
    }

    /// Finishes the chain and starts its call identifiers at one.
    #[must_use]
    pub fn build(self) -> Chain<S> {
        Chain {
            layers: self.layers,
            next_id: AtomicU64::new(1),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{Call, Chain, Completion, Denied, Direction, Layer, Outcome};

    #[derive(Default)]
    struct State {
        events: Vec<String>,
        body_runs: usize,
    }

    struct RecordingLayer {
        name: &'static str,
        deny: bool,
    }

    impl Layer<State> for RecordingLayer {
        type Frame = &'static str;

        fn before(&self, state: &mut State, _: &Call<'_>) -> Result<Self::Frame, Denied> {
            state.events.push(format!("before {}", self.name));
            if self.deny {
                Err(Denied::new(format!("{} refused", self.name)))
            } else {
                Ok(self.name)
            }
        }

        fn after(&self, state: &mut State, _: &Call<'_>, frame: Self::Frame, outcome: Outcome<'_>) {
            assert_eq!(frame, self.name);
            let outcome = match outcome {
                Outcome::Returned(completion) => format!("returned {:?}", completion.produced),
                Outcome::Failed(error) if error.downcast_ref::<Denied>().is_some() => {
                    "denied".to_owned()
                }
                Outcome::Failed(_) => "failed".to_owned(),
                Outcome::Cancelled => "cancelled".to_owned(),
            };
            state.events.push(format!("after {} {outcome}", self.name));
        }
    }

    fn call(id: u64) -> Call<'static> {
        Call {
            id,
            direction: Direction::Import,
            interface: Some("example:test/host"),
            function: "work",
            handles: &[],
            args: &(),
        }
    }

    #[test]
    fn hooks_are_an_onion_and_frames_return_to_their_layers() {
        let chain = Chain::builder()
            .layer(RecordingLayer {
                name: "outer",
                deny: false,
            })
            .layer(RecordingLayer {
                name: "inner",
                deny: false,
            })
            .build();
        let mut state = State::default();

        let value = chain
            .dispatch(&mut state, &call(chain.next_id()), |state| {
                state.body_runs += 1;
                Ok((42, Completion { produced: vec![7] }))
            })
            .unwrap();

        assert_eq!(value, 42);
        assert_eq!(state.body_runs, 1);
        assert_eq!(
            state.events,
            [
                "before outer",
                "before inner",
                "after inner returned [7]",
                "after outer returned [7]",
            ]
        );
    }

    #[test]
    fn denial_stops_before_body_and_inner_after_hooks() {
        let chain = Chain::builder()
            .layer(RecordingLayer {
                name: "outermost",
                deny: false,
            })
            .layer(RecordingLayer {
                name: "outer",
                deny: false,
            })
            .layer(RecordingLayer {
                name: "refusing",
                deny: true,
            })
            .layer(RecordingLayer {
                name: "inner",
                deny: false,
            })
            .build();
        let mut state = State::default();

        let error = chain
            .dispatch(&mut state, &call(chain.next_id()), |state| {
                state.body_runs += 1;
                Ok(((), Completion::default()))
            })
            .unwrap_err();

        assert_eq!(
            error.downcast_ref::<Denied>().unwrap().reason(),
            "refusing refused"
        );
        assert_eq!(state.body_runs, 0);
        assert_eq!(
            state.events,
            [
                "before outermost",
                "before outer",
                "before refusing",
                "after outer denied",
                "after outermost denied",
            ]
        );
    }

    #[test]
    fn body_failure_reaches_every_after_hook() {
        let chain = Chain::builder()
            .layer(RecordingLayer {
                name: "outer",
                deny: false,
            })
            .layer(RecordingLayer {
                name: "inner",
                deny: false,
            })
            .build();
        let mut state = State::default();

        let error = chain
            .dispatch::<()>(&mut state, &call(chain.next_id()), |state| {
                state.body_runs += 1;
                Err(wasmtime::Error::msg("handler failed"))
            })
            .unwrap_err();

        assert_eq!(error.to_string(), "handler failed");
        assert_eq!(
            state.events,
            [
                "before outer",
                "before inner",
                "after inner failed",
                "after outer failed",
            ]
        );
    }

    #[test]
    fn identifiers_are_unique_within_a_chain() {
        let chain = Chain::<State>::builder().build();

        assert_eq!(chain.next_id(), 1);
        assert_eq!(chain.next_id(), 2);
    }
}
