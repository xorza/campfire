use campfire_values::DeclaredName;
use derive_more::Display;

/// A way a modifier is applied, by design 04's list: with an action of its package, at each of
/// the action's ranks, or with no action, at rank 1.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Way {
    #[display("action {_0}")]
    Action(DeclaredName),
    #[display("no action")]
    NoAction,
}
