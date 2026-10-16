/// How a call applies the modifier it names, by the ways stats give (04-capabilities/stats.md, An
/// application never fails after its call): with the action of the call, at its rank, or with no
/// action, at rank 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    WithAction,
    WithoutAction,
}
