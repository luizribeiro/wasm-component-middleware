use std::collections::HashSet;

use crate::{Call, Denied, Layer, Outcome};

type Function = (Option<String>, String);

/// Allows only named component calls through a middleware chain.
///
/// Entries are fixed when the chain is built. Use [`allow_interface`](Self::allow_interface)
/// for every function in one interface, or [`allow_function`](Self::allow_function)
/// for one root or interface function.
#[derive(Default)]
pub struct Allowlist {
    interfaces: HashSet<String>,
    functions: HashSet<Function>,
}

impl Allowlist {
    /// Creates an empty allowlist that refuses every call.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Allows every function in `interface`.
    #[must_use]
    pub fn allow_interface(mut self, interface: impl Into<String>) -> Self {
        self.interfaces.insert(interface.into());
        self
    }

    /// Allows one function, using `None` for a root export.
    #[must_use]
    pub fn allow_function(mut self, interface: Option<&str>, function: impl Into<String>) -> Self {
        self.functions
            .insert((interface.map(str::to_owned), function.into()));
        self
    }

    fn allows(&self, call: &Call<'_>) -> bool {
        call.interface
            .is_some_and(|interface| self.interfaces.contains(interface))
            || self.functions.iter().any(|(interface, function)| {
                interface.as_deref() == call.interface && function == call.function
            })
    }
}

impl<S: 'static> Layer<S> for Allowlist {
    type Frame = ();

    fn before(&self, _state: &mut S, call: &Call<'_>) -> Result<(), Denied> {
        if self.allows(call) {
            Ok(())
        } else {
            let interface = call
                .interface
                .map_or(String::new(), |name| format!("{name}."));
            Err(Denied::new(format!(
                "{} {interface}{} is not allowed",
                call.direction, call.function
            )))
        }
    }

    fn after(&self, _state: &mut S, _call: &Call<'_>, (): (), _outcome: Outcome<'_>) {}
}

#[cfg(test)]
mod tests {
    use crate::{Allowlist, Call, Direction, Layer};

    fn call<'a>(direction: Direction, interface: Option<&'a str>, function: &'a str) -> Call<'a> {
        let call = Call::new(1, direction, function);
        match interface {
            Some(interface) => call.in_interface(interface, None),
            None => call,
        }
    }

    #[test]
    fn allows_named_function() {
        let layer = Allowlist::new().allow_function(Some("example:hello/host"), "user-name");

        assert!(
            layer
                .before(
                    &mut (),
                    &call(Direction::Import, Some("example:hello/host"), "user-name")
                )
                .is_ok()
        );
    }

    #[test]
    fn allows_whole_interface() {
        let layer = Allowlist::new().allow_interface("example:hello/host");

        assert!(
            layer
                .before(
                    &mut (),
                    &call(Direction::Import, Some("example:hello/host"), "log")
                )
                .is_ok()
        );
    }

    #[test]
    fn allows_every_version_of_an_interface() {
        let layer = Allowlist::new().allow_interface("wasi:clocks/wall-clock");
        let mut call = call(Direction::Import, Some("wasi:clocks/wall-clock"), "now");
        call.version = Some("0.2.12");

        assert!(layer.before(&mut (), &call).is_ok());
    }

    #[test]
    fn refuses_unnamed_function() {
        let layer = Allowlist::new().allow_function(Some("example:hello/host"), "log");
        let denied = layer
            .before(
                &mut (),
                &call(Direction::Import, Some("example:hello/host"), "user-name"),
            )
            .unwrap_err();

        assert_eq!(
            denied.reason(),
            "import example:hello/host.user-name is not allowed"
        );
    }

    #[test]
    fn allows_root_export() {
        let layer = Allowlist::new().allow_function(None, "greet");

        assert!(
            layer
                .before(&mut (), &call(Direction::Export, None, "greet"))
                .is_ok()
        );
    }

    #[test]
    fn does_not_match_another_interface() {
        let layer = Allowlist::new().allow_interface("example:other/host");

        assert!(
            layer
                .before(
                    &mut (),
                    &call(Direction::Import, Some("example:hello/host"), "log")
                )
                .is_err()
        );
    }
}
