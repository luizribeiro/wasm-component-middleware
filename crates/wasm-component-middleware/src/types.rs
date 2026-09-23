use std::error::Error;
use std::fmt::{self, Debug, Display};

/// Identifies which side of the component boundary initiated a call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// A host caller is invoking a component export.
    Export,
    /// A component is invoking a host import.
    Import,
}

impl Display for Direction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Export => formatter.write_str("export"),
            Self::Import => formatter.write_str("import"),
        }
    }
}

/// Describes one call observed by middleware.
#[derive(Debug)]
pub struct Call<'a> {
    /// The chain-local identifier used to correlate the call's two phases.
    pub id: u64,
    /// Whether the call enters or leaves the component.
    pub direction: Direction,
    /// The canonical WIT interface name, or `None` for a root export.
    pub interface: Option<&'a str>,
    /// The WIT function name.
    pub function: &'a str,
    /// Resource representations consumed by the call.
    pub handles: &'a [u32],
    /// A type-erased, read-only view used for diagnostics.
    ///
    /// The concrete shape of this field may change before version 1.0 as the
    /// component-value inspection API develops.
    pub args: &'a (dyn Debug + Send + Sync),
}

/// Describes resources produced by a completed call.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct Completion {
    /// Resource representations returned to the caller.
    pub produced: Vec<u32>,
}

/// Describes how a call ended when it reaches a layer's `after` hook.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum Outcome<'a> {
    /// The body returned successfully with completion metadata.
    Returned(&'a Completion),
    /// The body or an inner layer returned an error.
    Failed(&'a wasmtime::Error),
    /// The asynchronous body was dropped before it completed.
    Cancelled,
}

/// A policy refusal returned before the call body runs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Denied {
    reason: String,
}

impl Denied {
    /// Creates a refusal with a human-readable reason.
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }

    /// Returns the human-readable refusal reason.
    #[must_use]
    pub fn reason(&self) -> &str {
        &self.reason
    }
}

impl Display for Denied {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl Error for Denied {}

/// Brackets component calls with a decision phase and an observation phase.
pub trait Layer<S>: Send + Sync + 'static {
    /// State retained by this layer while the body runs.
    type Frame: Send + 'static;

    /// Allows the call and returns its frame, or refuses it before any effect.
    ///
    /// # Errors
    ///
    /// Returns [`Denied`] when policy refuses the call.
    fn before(&self, state: &mut S, call: &Call<'_>) -> Result<Self::Frame, Denied>;

    /// Observes the outcome of a call that this layer allowed.
    fn after(&self, state: &mut S, call: &Call<'_>, frame: Self::Frame, outcome: Outcome<'_>);
}

#[cfg(test)]
mod tests {
    use super::Denied;

    #[test]
    fn denial_survives_wasmtime_error_conversion() {
        let error = wasmtime::Error::from(Denied::new("user-name is disabled"));
        let denied = error.downcast_ref::<Denied>().unwrap();

        assert_eq!(denied.reason(), "user-name is disabled");
        assert_eq!(denied.to_string(), "user-name is disabled");
    }
}
