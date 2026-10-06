use campfire_capabilities::EngineEnum;
use campfire_script::ScriptError;
use thiserror::Error;

use crate::error::ctx_misuse::CtxMisuse;

/// What is wrong with a script.
#[derive(Debug, Error)]
pub enum ScriptProblem {
    /// No data names it.
    #[error("no data names it")]
    Unreferenced,
    /// Data names it, and the package does not hold it.
    #[error("named, but not held")]
    Missing,
    /// It does not compile.
    #[error(transparent)]
    Compile(ScriptError),
    /// A function named like a hook is no hook of a role the script serves, or takes another
    /// count of parameters.
    #[error("{0} is no hook of the script's roles")]
    UnknownHook(String),
    /// It uses a name on `ctx` that the script API does not define, or not for its role, or not
    /// in the way it uses it.
    #[error("ctx.{0} is not the script API's for this script")]
    UnknownCtx(String),
    /// It reads a field or calls a method no handle, no built-in and none of its own functions
    /// or object maps has.
    #[error(".{0} is no member the script API has")]
    UnknownMember(String),
    /// It reads or writes, after `.state`, a field that no state of the match declares: the
    /// mode's, a modifier's or a unit type's.
    #[error(".state.{0} is a field no state of the match declares")]
    UnknownState(String),
    /// It gives a string literal to an argument of `call` that takes a member of `takes`.
    #[error("{call} takes a member of {takes}, not a string")]
    EnumString { call: String, takes: EngineEnum },
    /// It reads or calls `path`, which is neither a member of `of` nor its function.
    #[error("{path} is no member of {of}, nor its function")]
    UnknownEnumMember { path: String, of: EngineEnum },
    /// It makes a function pointer: a closure, an anonymous function or a call of `Fn`.
    #[error("makes a function pointer: a closure, an anonymous function or Fn")]
    FunctionPointer,
    /// It uses `ctx` other than design 08's convention allows, so the load checks cannot see
    /// every use of it.
    #[error(transparent)]
    CtxMisuse(CtxMisuse),
    /// It uses or defines a name design 08 plans, a `ctx` name, a field or method of a handle, or
    /// a hook, which the release does not run yet.
    #[error("{0} is planned, and the release does not run it yet")]
    Planned(String),
}
