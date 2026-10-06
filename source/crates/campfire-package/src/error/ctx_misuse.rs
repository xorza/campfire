use std::fmt;

/// A use of `ctx` that hides it from the load checks: every value of `ctx` in a script is a
/// variable named `ctx`, used as `ctx.<name>` or as a whole argument of a call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CtxMisuse {
    /// `ctx` is used other than as `ctx.<name>` or as a whole argument of a call, or is given to
    /// an operator.
    Stray,
    /// The script's own function receives `ctx` under another parameter name.
    Renamed { function: String },
    /// A `let`, a `const` or a `for` binds a new variable named `ctx`.
    Bound,
    /// A hook's first parameter is not named `ctx`.
    HookParam { function: String },
}

impl fmt::Display for CtxMisuse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CtxMisuse::Stray => f.write_str("ctx used other than as ctx.<name> or a call argument"),
            CtxMisuse::Renamed { function } => {
                write!(f, "{function} receives ctx under another name")
            }
            CtxMisuse::Bound => f.write_str("binds a new variable named ctx"),
            CtxMisuse::HookParam { function } => {
                write!(f, "the first parameter of {function} is not ctx")
            }
        }
    }
}
