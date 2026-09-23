use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::{Call, Denied, Layer, MiddlewareView, Outcome};

/// A layer that writes one human-readable line before and after every call.
///
/// Indentation is the number of calls this logger already has open for the
/// same invocation, so overlapping sibling calls remain balanced even when
/// they finish out of order.
pub struct Logger<W = io::Stderr> {
    writer: Arc<Mutex<W>>,
    depths: Arc<Mutex<HashMap<u64, usize>>>,
}

impl<W> Clone for Logger<W> {
    fn clone(&self) -> Self {
        Self {
            writer: Arc::clone(&self.writer),
            depths: Arc::clone(&self.depths),
        }
    }
}

impl<W> Logger<W> {
    /// Creates a logger that owns `writer` behind a shareable lock.
    pub fn new(writer: W) -> Self {
        Self {
            writer: Arc::new(Mutex::new(writer)),
            depths: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Returns the shared writer, which is useful for capturing test output.
    #[must_use]
    pub fn writer(&self) -> Arc<Mutex<W>> {
        Arc::clone(&self.writer)
    }

    fn lock_writer(&self) -> MutexGuard<'_, W> {
        match self.writer.lock() {
            Ok(writer) => writer,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    fn begin_call(&self, invocation: u64) -> usize {
        let mut depths = match self.depths.lock() {
            Ok(depths) => depths,
            Err(poisoned) => poisoned.into_inner(),
        };
        let depth = depths.entry(invocation).or_default();
        let current = *depth;
        *depth = depth.saturating_add(1);
        current
    }

    fn end_call(&self, invocation: u64) {
        let mut depths = match self.depths.lock() {
            Ok(depths) => depths,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(depth) = depths.get_mut(&invocation) {
            *depth = depth.saturating_sub(1);
            if *depth == 0 {
                depths.remove(&invocation);
            }
        }
    }
}

impl Logger<io::Stderr> {
    /// Creates a logger that writes to standard error.
    #[must_use]
    pub fn stderr() -> Self {
        Self::new(io::stderr())
    }
}

impl Default for Logger<io::Stderr> {
    fn default() -> Self {
        Self::stderr()
    }
}

impl<S, W> Layer<S> for Logger<W>
where
    S: MiddlewareView + 'static,
    W: Write + Send + 'static,
{
    type Frame = (u64, usize);

    fn before(&self, state: &mut S, call: &Call<'_>) -> Result<Self::Frame, Denied> {
        let invocation = state.middleware().context().id();
        let depth = self.begin_call(invocation);

        let args = format!("{:?}", call.args);
        let args = if args == "()" { "" } else { &args };
        let interface = call.interface.map_or(String::new(), |interface| {
            call.version.map_or_else(
                || format!("{interface}."),
                |version| format!("{interface}@{version}."),
            )
        });
        let _ = writeln!(
            self.lock_writer(),
            "{}→ #{} {} {interface}{}({args})",
            "  ".repeat(depth),
            call.id,
            call.direction,
            call.function,
        );
        Ok((invocation, depth))
    }

    fn after(&self, _state: &mut S, call: &Call<'_>, frame: Self::Frame, outcome: Outcome<'_>) {
        let (invocation, depth) = frame;
        self.end_call(invocation);
        let outcome = match outcome {
            Outcome::Returned(_) => "returned".to_owned(),
            Outcome::Failed(error) => error.downcast_ref::<Denied>().map_or_else(
                || format!("failed: {error}"),
                |denied| format!("failed: {denied}"),
            ),
            Outcome::Cancelled => "cancelled".to_owned(),
        };
        let _ = writeln!(
            self.lock_writer(),
            "{}← #{} {outcome}",
            "  ".repeat(depth),
            call.id,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        Call, Chain, Completion, Direction, InvocationContext, Logger, MiddlewareCtx,
        MiddlewareView, Outcome,
    };

    struct State {
        middleware: Option<MiddlewareCtx<Self>>,
    }

    impl MiddlewareView for State {
        fn middleware(&mut self) -> &mut MiddlewareCtx<Self> {
            self.middleware.as_mut().unwrap()
        }
    }

    #[test]
    fn indents_an_import_nested_inside_an_export() {
        let logger = Logger::new(Vec::new());
        let output = logger.writer();
        let chain = Arc::new(Chain::builder().layer(logger).build());
        let mut state = State {
            middleware: Some(MiddlewareCtx::new(
                Arc::clone(&chain),
                InvocationContext::new("greeter"),
            )),
        };
        let export = Call {
            id: chain.next_id(),
            direction: Direction::Export,
            interface: None,
            version: None,
            function: "greet",
            handles: &[],
            args: &"Hello",
        };

        chain
            .dispatch(&mut state, &export, |state| {
                let import = Call {
                    id: chain.next_id(),
                    direction: Direction::Import,
                    interface: Some("example:hello/host"),
                    version: Some("1.0.0"),
                    function: "user-name",
                    handles: &[],
                    args: &(),
                };
                chain.dispatch(state, &import, |_| Ok(((), Completion::default())))?;
                Ok(((), Completion::default()))
            })
            .unwrap();

        let bytes = output.lock().unwrap().clone();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            concat!(
                "→ #1 export greet(\"Hello\")\n",
                "  → #2 import example:hello/host@1.0.0.user-name()\n",
                "  ← #2 returned\n",
                "← #1 returned\n",
            )
        );
    }

    #[test]
    fn overlapping_calls_finish_out_of_order_without_stale_depth() {
        let logger = Logger::new(Vec::new());
        let output = logger.writer();
        let chain = Arc::new(Chain::builder().layer(logger).build());
        let mut state = State {
            middleware: Some(MiddlewareCtx::new(
                Arc::clone(&chain),
                InvocationContext::new("greeter"),
            )),
        };
        let first = Call {
            id: chain.next_id(),
            direction: Direction::Import,
            interface: None,
            version: None,
            function: "first",
            handles: &[],
            args: &(),
        };
        let second = Call {
            id: chain.next_id(),
            direction: Direction::Import,
            interface: None,
            version: None,
            function: "second",
            handles: &[],
            args: &(),
        };
        let (first_frames, first_denial) = chain.before(&mut state, &first);
        let (second_frames, second_denial) = chain.before(&mut state, &second);
        let completion = Completion::default();

        assert!(first_denial.is_none());
        assert!(second_denial.is_none());
        Chain::after(
            &mut state,
            &first,
            first_frames,
            Outcome::Returned(&completion),
        );
        Chain::after(
            &mut state,
            &second,
            second_frames,
            Outcome::Returned(&completion),
        );
        let third = Call {
            id: chain.next_id(),
            direction: Direction::Import,
            interface: None,
            version: None,
            function: "third",
            handles: &[],
            args: &(),
        };
        chain
            .dispatch(&mut state, &third, |_| Ok(((), Completion::default())))
            .unwrap();

        let bytes = output.lock().unwrap().clone();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            concat!(
                "→ #1 import first()\n",
                "  → #2 import second()\n",
                "← #1 returned\n",
                "  ← #2 returned\n",
                "→ #3 import third()\n",
                "← #3 returned\n",
            )
        );
    }

    #[test]
    fn logger_instances_track_depth_independently() {
        let outer = Logger::new(Vec::new());
        let outer_output = outer.writer();
        let inner = Logger::new(Vec::new());
        let inner_output = inner.writer();
        let chain = Arc::new(Chain::builder().layer(outer).layer(inner).build());
        let mut state = State {
            middleware: Some(MiddlewareCtx::new(
                Arc::clone(&chain),
                InvocationContext::new("greeter"),
            )),
        };
        let call = Call {
            id: chain.next_id(),
            direction: Direction::Export,
            interface: None,
            version: None,
            function: "greet",
            handles: &[],
            args: &"Hello",
        };

        chain
            .dispatch(&mut state, &call, |_| Ok(((), Completion::default())))
            .unwrap();

        for output in [outer_output, inner_output] {
            let bytes = output.lock().unwrap().clone();
            assert_eq!(
                String::from_utf8(bytes).unwrap(),
                "→ #1 export greet(\"Hello\")\n← #1 returned\n"
            );
        }
    }
}
