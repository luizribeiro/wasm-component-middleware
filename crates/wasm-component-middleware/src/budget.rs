use std::collections::HashMap;
use std::hash::Hash;
use std::sync::{Mutex, MutexGuard};

use crate::{Call, Denied, Layer, Outcome};

/// A layer that shares a fixed allowance for each selected key.
///
/// Use this layer for process-wide quotas that must span every store using the
/// same [`crate::Chain`]. The selector returns a key and quantity for calls
/// that consume the allowance, or `None` for calls that do not.
pub struct Budget<K, F> {
    limit: u64,
    used: Mutex<HashMap<K, u64>>,
    selector: F,
}

impl<K, F> Budget<K, F> {
    /// Creates a budget with `limit` units available independently per key.
    pub fn new(limit: u64, selector: F) -> Self {
        Self {
            limit,
            used: Mutex::new(HashMap::new()),
            selector,
        }
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<K, u64>> {
        self.used
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<S, K, F> Layer<S> for Budget<K, F>
where
    K: Eq + Hash + Send + 'static,
    F: Fn(&Call<'_>) -> Option<(K, u64)> + Send + Sync + 'static,
{
    type Frame = ();

    fn before(&self, _state: &mut S, call: &Call<'_>) -> Result<(), Denied> {
        let Some((key, quantity)) = (self.selector)(call) else {
            return Ok(());
        };
        let mut usage = self.lock();
        let used = usage.entry(key).or_default();
        let updated = used.saturating_add(quantity);
        if updated > self.limit || (quantity != 0 && *used == self.limit) {
            return Err(Denied::new("budget exhausted"));
        }
        *used = updated;
        Ok(())
    }

    fn after(&self, _state: &mut S, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

#[cfg(test)]
mod tests {
    use crate::{Budget, Call, Direction, Layer};

    #[test]
    fn quantities_are_counted_independently_per_key() {
        let budget = Budget::new(5, |call: &Call<'_>| {
            call.args
                .get("bytes")
                .and_then(crate::ArgumentValue::as_u64)
                .map(|quantity| (call.function.to_owned(), quantity))
        });
        let mut state = ();
        let args = crate::Arguments::new().with("bytes", 3_u64);
        let alpha = Call::new(1, Direction::Import, "alpha").with_args(&args);
        let beta = Call::new(2, Direction::Import, "beta").with_args(&args);

        assert!(Layer::before(&budget, &mut state, &alpha).is_ok());
        assert!(Layer::before(&budget, &mut state, &alpha).is_err());
        assert!(Layer::before(&budget, &mut state, &beta).is_ok());
    }

    #[test]
    fn a_saturated_counter_refuses_more_usage() {
        let budget = Budget::new(u64::MAX, |_call: &Call<'_>| Some(((), u64::MAX)));
        let mut state = ();
        let call = Call::new(1, Direction::Import, "bytes");

        assert!(Layer::before(&budget, &mut state, &call).is_ok());
        assert!(Layer::before(&budget, &mut state, &call).is_err());
    }
}
