use thiserror::Error;

/// A use of `ctx` that hides it from the load checks: every value of `ctx` in a script is a
/// variable named `ctx`, used as `ctx.<name>` or as a whole argument of a call.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CtxMisuse {
    /// `ctx` is used other than as `ctx.<name>` or as a whole argument of a call, or is given to
    /// an operator.
    #[error("ctx used other than as ctx.<name> or a call argument")]
    Stray,
    /// The script's own function receives `ctx` under another parameter name.
    #[error("{function} receives ctx under another name")]
    Renamed { function: String },
    /// A `let`, a `const` or a `for` binds a new variable named `ctx`.
    #[error("binds a new variable named ctx")]
    Bound,
    /// A hook's first parameter is not named `ctx`.
    #[error("the first parameter of {function} is not ctx")]
    HookParam { function: String },
}
