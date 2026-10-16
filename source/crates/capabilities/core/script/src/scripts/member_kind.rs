/// Rhai's name for a property's getter, which its `register_get` and kin use and do not export.
pub(crate) const GETTER: &str = "get$";

/// How a script uses a name: reads a value of `ctx`, calls `ctx`, reads a handle's field, calls
/// a handle's method, or applies an operator to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Value,
    Call,
    Field,
    Method,
    Operator,
}

impl MemberKind {
    /// The name Rhai calls a member `name` of this kind by: its getter's, for a value or a field.
    pub(crate) fn bound_name(self, name: &str) -> String {
        match self {
            Self::Value | Self::Field => format!("{GETTER}{name}"),
            Self::Call | Self::Method | Self::Operator => name.to_owned(),
        }
    }
}
