use campfire_common::DeclaredName;
use derive_more::Display;

/// A way a modifier is applied, by the ways stats give (04-capabilities/stats.md, An application
/// never fails after its call): with an action of its package, at each of the action's ranks, or
/// with no action, at rank 1.
#[derive(Debug, Display, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Way {
    #[display("action {_0}")]
    Action(DeclaredName),
    #[display("no action")]
    NoAction,
}
