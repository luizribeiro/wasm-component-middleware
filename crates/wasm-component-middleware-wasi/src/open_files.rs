use std::collections::BTreeSet;

use wasm_component_middleware::{Call, Denied, Layer, MiddlewareView, Outcome};

const FILESYSTEM_TYPES: &str = "wasi:filesystem/types";
const FILESYSTEM_PREOPENS: &str = "wasi:filesystem/preopens";
const OPEN_AT: &str = "[method]descriptor.open-at";
const DROP_DESCRIPTOR: &str = "[resource-drop]descriptor";

#[derive(Default)]
struct OpenFileState {
    live: BTreeSet<u32>,
    pending: usize,
}

/// Limits the number of live WASI filesystem descriptors in an invocation.
///
/// Preopened directories count toward the limit. A descriptor stops counting
/// when its `[resource-drop]descriptor` call begins, and a failed `open-at`
/// does not consume a slot.
pub struct OpenFiles {
    limit: usize,
}

impl OpenFiles {
    /// Creates a layer that refuses `open-at` once `limit` descriptors are live
    /// or reserved by calls in progress.
    #[must_use]
    pub const fn new(limit: usize) -> Self {
        Self { limit }
    }
}

impl<S> Layer<S> for OpenFiles
where
    S: MiddlewareView + 'static,
{
    type Frame = Option<bool>;

    fn before(&self, state: &mut S, call: &Call<'_>) -> Result<Self::Frame, Denied> {
        let filesystem_types = call.interface == Some(FILESYSTEM_TYPES);
        let context = state.middleware().context_mut();
        if context.get::<OpenFileState>().is_none() {
            context.insert(OpenFileState::default());
        }
        let open_files = context
            .get_mut::<OpenFileState>()
            .ok_or_else(|| Denied::new("open-file state is unavailable"))?;

        if filesystem_types && call.function == DROP_DESCRIPTOR {
            for handle in call.handles {
                open_files.live.remove(handle);
            }
            return Ok(None);
        }
        if filesystem_types && call.function == OPEN_AT {
            if open_files.live.len() + open_files.pending >= self.limit {
                return Err(Denied::new(format!(
                    "open file limit of {} reached",
                    self.limit
                )));
            }
            open_files.pending += 1;
            return Ok(Some(true));
        }
        if call.interface == Some(FILESYSTEM_PREOPENS) && call.function == "get-directories" {
            return Ok(Some(false));
        }
        Ok(None)
    }

    fn after(&self, state: &mut S, _call: &Call<'_>, frame: Self::Frame, outcome: Outcome<'_>) {
        let Some(open_files) = state.middleware().context_mut().get_mut::<OpenFileState>() else {
            return;
        };
        if frame == Some(true) {
            open_files.pending = open_files.pending.saturating_sub(1);
        }
        if frame.is_some()
            && let Outcome::Returned(completion) = outcome
        {
            open_files.live.extend(&completion.produced);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use wasm_component_middleware::{
        Chain, Completion, Direction, InvocationContext, MiddlewareCtx, StateAccess,
    };

    use super::*;

    struct State {
        middleware: MiddlewareCtx<Self>,
    }

    impl MiddlewareView for State {
        fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
            &mut self.middleware
        }
    }

    fn state(chain: Arc<Chain<State>>) -> State {
        State {
            middleware: MiddlewareCtx::new(chain, InvocationContext::new("files")),
        }
    }

    fn call<'a>(function: &'a str, handles: &'a [u32]) -> Call<'a> {
        Call::new(1, Direction::Import, function)
            .in_interface(FILESYSTEM_TYPES, Some("0.2.12"))
            .with_handles(handles)
    }

    #[test]
    fn a_drop_frees_a_slot() {
        let layer = OpenFiles::new(1);
        let chain = Chain::builder().build();
        let mut state = state(chain);
        let open = call(OPEN_AT, &[3]);
        let frame = layer.before(&mut state, &open).unwrap();
        let completion = Completion { produced: vec![4] };
        layer.after(&mut state, &open, frame, Outcome::Returned(&completion));
        assert!(layer.before(&mut state, &open).is_err());

        let drop = call(DROP_DESCRIPTOR, &[4]);
        layer.before(&mut state, &drop).unwrap();

        assert!(layer.before(&mut state, &open).is_ok());
    }

    #[test]
    fn a_failed_open_releases_its_reservation() {
        let layer = OpenFiles::new(1);
        let chain = Chain::builder().build();
        let mut state = state(chain);
        let open = call(OPEN_AT, &[3]);
        let frame = layer.before(&mut state, &open).unwrap();
        let error = wasmtime::Error::msg("missing file");
        layer.after(&mut state, &open, frame, Outcome::Failed(&error));

        assert!(layer.before(&mut state, &open).is_ok());
    }

    struct SharedState(Mutex<State>);

    impl StateAccess<State> for &SharedState {
        fn with_state<R>(&mut self, operation: impl FnOnce(&mut State) -> R) -> R {
            operation(&mut self.0.lock().unwrap())
        }
    }

    #[tokio::test]
    async fn concurrent_p3_opens_share_the_last_slot() {
        let chain = Chain::builder().layer(OpenFiles::new(2)).build();
        let mut initial = state(Arc::clone(&chain));
        let preopens = Call::new(1, Direction::Import, "get-directories")
            .in_interface(FILESYSTEM_PREOPENS, Some("0.3.0"));
        chain
            .dispatch(&mut initial, &preopens, |_| {
                Ok(((), Completion { produced: vec![1] }))
            })
            .unwrap();
        let shared = SharedState(Mutex::new(initial));
        let handles = [1];
        let first = Call::new(2, Direction::Import, OPEN_AT)
            .in_interface(FILESYSTEM_TYPES, Some("0.3.0"))
            .with_handles(&handles);
        let second = Call::new(3, Direction::Import, OPEN_AT)
            .in_interface(FILESYSTEM_TYPES, Some("0.3.0"))
            .with_handles(&handles);

        let first_open = chain.dispatch_async(&shared, &first, || async {
            tokio::task::yield_now().await;
            Ok(((), Completion { produced: vec![2] }))
        });
        let second_open = chain.dispatch_async(&shared, &second, || async {
            tokio::task::yield_now().await;
            Ok(((), Completion { produced: vec![3] }))
        });
        let (first_result, second_result) = tokio::join!(first_open, second_open);

        assert!(first_result.is_ok());
        assert_eq!(
            second_result
                .unwrap_err()
                .downcast_ref::<Denied>()
                .unwrap()
                .reason(),
            "open file limit of 2 reached"
        );
    }
}
