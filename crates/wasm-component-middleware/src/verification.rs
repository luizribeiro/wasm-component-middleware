use std::collections::HashSet;
use std::error::Error;
use std::fmt::{self, Display};

use wasmtime::Engine;
use wasmtime::component::Component;
use wasmtime::component::types::ComponentItem;

use crate::RoutedInterface;

/// Lists component functions that are not routed through middleware.
#[derive(Debug, Eq, PartialEq)]
pub struct Unrouted {
    functions: Vec<String>,
}

impl Unrouted {
    /// Returns every unrouted component function.
    #[must_use]
    pub fn functions(&self) -> &[String] {
        &self.functions
    }
}

impl Display for Unrouted {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "imports {} are not routed through the middleware chain",
            self.functions.join(", ")
        )
    }
}

impl Error for Unrouted {}

/// Checks component imports against the supplied routed-interface markers.
///
/// This verifies the declared routing set, not a linker's actual wiring. Gate
/// dispatch coverage tests should separately prove that each marked interface
/// is wired through middleware. `unchecked_prefixes` explicitly exempts
/// imports that are not routed yet. The caller should use Wasmtime's
/// `Linker::define_unknown_imports_as_traps` after registration as a backstop
/// for imports that are intentionally unavailable.
///
/// # Errors
///
/// Returns an error naming every checked function whose interface was not
/// emitted by [`route_imports!`](crate::route_imports).
pub fn verify_routing<'a>(
    engine: &Engine,
    component: &Component,
    routed: impl IntoIterator<Item = RoutedInterface>,
    unchecked_prefixes: impl IntoIterator<Item = &'a str>,
) -> Result<(), Unrouted> {
    let routed = routed
        .into_iter()
        .map(RoutedInterface::name)
        .collect::<HashSet<_>>();
    let unchecked_prefixes = unchecked_prefixes.into_iter().collect::<Vec<_>>();

    let mut functions = Vec::new();
    for (name, import) in component.component_type().imports(engine) {
        if unchecked_prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        match import.ty {
            ComponentItem::ComponentInstance(_) if routed.contains(unversioned_interface(name)) => {
            }
            ComponentItem::ComponentInstance(instance) => {
                functions.extend(instance.exports(engine).filter_map(|(function, export)| {
                    match export.ty {
                        ComponentItem::ComponentFunc(_) => Some(format!("{name}.{function}")),
                        ComponentItem::Resource(_) => {
                            Some(format!("{name}.[resource-drop]{function}"))
                        }
                        _ => None,
                    }
                }));
            }
            _ => {
                functions.push(name.to_owned());
            }
        }
    }

    if functions.is_empty() {
        Ok(())
    } else {
        Err(Unrouted { functions })
    }
}

fn unversioned_interface(name: &str) -> &str {
    name.rsplit_once('@')
        .map_or(name, |(interface, _)| interface)
}
