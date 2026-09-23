use std::error::Error;
use std::fmt::{self, Display};

static EMPTY_ARGUMENTS: Arguments = Arguments(Vec::new());

/// Maximum number of bytes retained for a byte-list argument snapshot.
pub const BYTE_ARGUMENT_PREFIX_LEN: usize = 64;

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
#[non_exhaustive]
pub struct Call<'a> {
    /// The chain-local identifier used to correlate the call's two phases.
    pub id: u64,
    /// Whether the call enters or leaves the component.
    pub direction: Direction,
    /// The unversioned WIT interface name, or `None` for a root export.
    pub interface: Option<&'a str>,
    /// The WIT interface version when the call belongs to a versioned interface.
    pub version: Option<&'a str>,
    /// The WIT function name.
    pub function: &'a str,
    /// Resource representations consumed by the call.
    pub handles: &'a [u32],
    /// A typed, read-only view of the call's non-resource arguments.
    pub args: &'a Arguments,
}

#[derive(Debug)]
pub(crate) struct OwnedCall {
    id: u64,
    direction: Direction,
    interface: Option<String>,
    version: Option<String>,
    function: String,
    handles: Vec<u32>,
    args: Arguments,
}

impl OwnedCall {
    pub(crate) fn from_call(call: &Call<'_>) -> Self {
        Self {
            id: call.id,
            direction: call.direction,
            interface: call.interface.map(str::to_owned),
            version: call.version.map(str::to_owned),
            function: call.function.to_owned(),
            handles: call.handles.to_vec(),
            args: call.args.clone(),
        }
    }

    pub(crate) fn as_call(&self) -> Call<'_> {
        Call {
            id: self.id,
            direction: self.direction,
            interface: self.interface.as_deref(),
            version: self.version.as_deref(),
            function: &self.function,
            handles: &self.handles,
            args: &self.args,
        }
    }
}

impl<'a> Call<'a> {
    /// Describes a call with no interface, resource handles, or arguments.
    #[must_use]
    pub const fn new(id: u64, direction: Direction, function: &'a str) -> Self {
        Self {
            id,
            direction,
            interface: None,
            version: None,
            function,
            handles: &[],
            args: &EMPTY_ARGUMENTS,
        }
    }

    /// Associates the call with a WIT interface and optional version.
    #[must_use]
    pub const fn in_interface(mut self, interface: &'a str, version: Option<&'a str>) -> Self {
        self.interface = Some(interface);
        self.version = version;
        self
    }

    /// Records resource representations consumed by the call.
    #[must_use]
    pub const fn with_handles(mut self, handles: &'a [u32]) -> Self {
        self.handles = handles;
        self
    }

    /// Records a typed view of the call's non-resource arguments.
    #[must_use]
    pub const fn with_args(mut self, args: &'a Arguments) -> Self {
        self.args = args;
        self
    }
}

/// A named, typed view of a call's non-resource arguments.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Arguments(Vec<Argument>);

impl Arguments {
    /// Creates an empty argument view.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// Adds a named argument.
    #[must_use]
    pub fn with(mut self, name: &'static str, value: impl Into<ArgumentValue>) -> Self {
        self.0.push(Argument {
            name,
            value: value.into(),
        });
        self
    }

    /// Adds a diagnostic-only argument when no typed conversion is available.
    #[must_use]
    pub fn with_debug(mut self, name: &'static str, value: &impl fmt::Debug) -> Self {
        self.0.push(Argument {
            name,
            value: ArgumentValue::Debug(format!("{value:?}")),
        });
        self
    }

    /// Returns the value of the named argument.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ArgumentValue> {
        self.0
            .iter()
            .find(|argument| argument.name == name)
            .map(|argument| &argument.value)
    }

    /// Iterates over the arguments in WIT parameter order.
    #[must_use]
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&str, &ArgumentValue)> {
        self.0
            .iter()
            .map(|argument| (argument.name, &argument.value))
    }

    /// Returns whether there are no non-resource arguments.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Display for Arguments {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, argument) in self.0.iter().enumerate() {
            if index != 0 {
                formatter.write_str(", ")?;
            }
            write!(formatter, "{}={}", argument.name, argument.value)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Argument {
    name: &'static str,
    value: ArgumentValue,
}

/// A policy-readable component value copied before owned parameters are moved.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ArgumentValue {
    /// A Boolean value.
    Bool(bool),
    /// A signed integer value.
    Signed(i64),
    /// An unsigned integer value.
    Unsigned(u64),
    /// A string value.
    String(String),
    /// A byte list represented by a bounded prefix and its original length.
    Bytes {
        /// Bytes retained for policy inspection.
        ///
        /// [`Self::bytes`] caps this data at [`BYTE_ARGUMENT_PREFIX_LEN`],
        /// while [`Self::owned_bytes`] retains an already-owned buffer in full.
        prefix: Vec<u8>,
        /// The byte list's original length.
        total_len: usize,
    },
    /// A list of component values.
    List(Vec<Self>),
    /// A variant case with an optional payload.
    Variant {
        /// The WIT case name.
        case: &'static str,
        /// The case payload, when present.
        value: Option<Box<Self>>,
    },
    /// A diagnostic representation for values without a typed projection.
    Debug(String),
}

impl ArgumentValue {
    /// Creates a bounded byte-list snapshot.
    ///
    /// At most [`BYTE_ARGUMENT_PREFIX_LEN`] bytes are copied, while
    /// [`Self::byte_len`] reports the original length.
    #[must_use]
    pub fn bytes(value: impl AsRef<[u8]>) -> Self {
        let value = value.as_ref();
        Self::Bytes {
            prefix: value
                .iter()
                .take(BYTE_ARGUMENT_PREFIX_LEN)
                .copied()
                .collect(),
            total_len: value.len(),
        }
    }

    /// Creates a byte-list argument from data the caller already owns.
    ///
    /// Unlike [`Self::bytes`], this retains the whole buffer. Use it when the
    /// data has already been copied for another purpose, such as a relayed
    /// stream chunk, and layers need to inspect every byte.
    #[must_use]
    pub fn owned_bytes(value: Vec<u8>) -> Self {
        let total_len = value.len();
        Self::Bytes {
            prefix: value,
            total_len,
        }
    }

    /// Creates a variant case without a payload.
    #[must_use]
    pub const fn case(case: &'static str) -> Self {
        Self::Variant { case, value: None }
    }

    /// Returns this value as a string when it has string type.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    /// Returns the retained bytes when this value has byte-list type.
    #[must_use]
    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Bytes { prefix, .. } => Some(prefix),
            _ => None,
        }
    }

    /// Returns the original byte-list length when this value has byte-list type.
    #[must_use]
    pub const fn byte_len(&self) -> Option<usize> {
        match self {
            Self::Bytes { total_len, .. } => Some(*total_len),
            _ => None,
        }
    }

    /// Returns this value as an unsigned integer when possible.
    #[must_use]
    pub const fn as_u64(&self) -> Option<u64> {
        match self {
            Self::Unsigned(value) => Some(*value),
            _ => None,
        }
    }
}

impl Display for ArgumentValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Bool(value) => Display::fmt(value, formatter),
            Self::Signed(value) => Display::fmt(value, formatter),
            Self::Unsigned(value) => Display::fmt(value, formatter),
            Self::String(value) => write!(formatter, "{value:?}"),
            Self::Bytes { prefix, total_len } => {
                const PREVIEW: usize = 24;
                let preview = prefix
                    .iter()
                    .take(PREVIEW)
                    .map(|byte| match byte {
                        b' '..=b'~' => char::from(*byte),
                        _ => '.',
                    })
                    .collect::<String>();
                let ellipsis = if *total_len > PREVIEW { "…" } else { "" };
                write!(formatter, "{total_len} bytes \"{preview}{ellipsis}\"")
            }
            Self::List(values) => {
                formatter.write_str("[")?;
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        formatter.write_str(", ")?;
                    }
                    Display::fmt(value, formatter)?;
                }
                formatter.write_str("]")
            }
            Self::Variant { case, value: None } => formatter.write_str(case),
            Self::Variant {
                case,
                value: Some(value),
            } => write!(formatter, "{case}({value})"),
            Self::Debug(value) => formatter.write_str(value),
        }
    }
}

impl From<bool> for ArgumentValue {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

macro_rules! integer_values {
    ($variant:ident: $($type:ty),+ $(,)?) => {
        $(
            impl From<$type> for ArgumentValue {
                fn from(value: $type) -> Self {
                    Self::$variant(value.into())
                }
            }
        )+
    };
}

integer_values!(Unsigned: u8, u16, u32, u64);
integer_values!(Signed: i8, i16, i32, i64);

impl From<String> for ArgumentValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<&str> for ArgumentValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
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
    ///
    /// An `after` hook sees this outcome once the store is next available, so
    /// delivery can happen later than the future's drop. If the store is
    /// dropped first, the hook is not called.
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
    use super::{ArgumentValue, Arguments, BYTE_ARGUMENT_PREFIX_LEN, Denied};

    #[test]
    fn typed_arguments_support_policy_lookup() {
        let args = Arguments::new()
            .with("path", "config/settings.toml")
            .with("bytes", ArgumentValue::bytes(b"hello"));

        assert_eq!(
            args.get("path").and_then(ArgumentValue::as_str),
            Some("config/settings.toml")
        );
        assert_eq!(
            args.get("bytes").and_then(ArgumentValue::as_bytes),
            Some(b"hello".as_slice())
        );
    }

    #[test]
    fn byte_arguments_retain_a_bounded_prefix_and_total_length() {
        let bytes = (0..BYTE_ARGUMENT_PREFIX_LEN + 17)
            .map(|value| u8::try_from(value % 256).unwrap())
            .collect::<Vec<_>>();
        let value = ArgumentValue::bytes(&bytes);

        assert_eq!(value.as_bytes(), Some(&bytes[..BYTE_ARGUMENT_PREFIX_LEN]));
        assert_eq!(value.byte_len(), Some(bytes.len()));
    }

    #[test]
    fn owned_byte_arguments_retain_the_complete_buffer() {
        let bytes = vec![7; BYTE_ARGUMENT_PREFIX_LEN + 17];
        let value = ArgumentValue::owned_bytes(bytes.clone());

        assert_eq!(value.as_bytes(), Some(bytes.as_slice()));
        assert_eq!(value.byte_len(), Some(bytes.len()));
    }

    #[test]
    fn denial_survives_wasmtime_error_conversion() {
        let error = wasmtime::Error::from(Denied::new("user-name is disabled"));
        let denied = error.downcast_ref::<Denied>().unwrap();

        assert_eq!(denied.reason(), "user-name is disabled");
        assert_eq!(denied.to_string(), "user-name is disabled");
    }
}
