use crate::scripts::state_decl::StateType;

/// A field of a modifier's script state, as its spec declares it: its name and type, in the order
/// of the names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModifierStateField {
    pub(crate) name: Box<str>,
    pub(crate) kind: StateType,
}
