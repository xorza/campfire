/// How a call applies the modifier it names, by design 04's ways: with the action of the call,
/// at its rank, or with no action, at rank 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applies {
    WithAction,
    WithoutAction,
}
