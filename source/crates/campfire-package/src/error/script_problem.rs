use std::fmt;

use campfire_capabilities::EngineEnum;
use campfire_script::ScriptError;

use crate::error::ctx_misuse::CtxMisuse;

/// What is wrong with a script.
#[derive(Debug)]
pub enum ScriptProblem {
    /// No data names it.
    Unreferenced,
    /// Data names it, and the package does not hold it.
    Missing,
    /// It does not compile.
    Compile(ScriptError),
    /// A function named like a hook is no hook of a role the script serves, or takes another
    /// count of parameters.
    UnknownHook(String),
    /// It uses a name on `ctx` that the script API does not define, or not for its role, or not
    /// in the way it uses it.
    UnknownCtx(String),
    /// It reads a field or calls a method no handle, no built-in and none of its own functions
    /// or object maps has.
    UnknownMember(String),
    /// It reads or writes, after `.state`, a field that no state of the match declares: the
    /// mode's, a modifier's or a unit type's.
    UnknownState(String),
    /// It gives a string literal to an argument of `call` that takes a member of `takes`.
    EnumString { call: String, takes: EngineEnum },
    /// It reads or calls `path`, which is neither a member of `of` nor its function.
    UnknownEnumMember { path: String, of: EngineEnum },
    /// It makes a function pointer: a closure, an anonymous function or a call of `Fn`.
    FunctionPointer,
    /// It uses `ctx` other than design 08's convention allows, so the load checks cannot see
    /// every use of it.
    CtxMisuse(CtxMisuse),
    /// It uses or defines a name design 08 plans, a `ctx` name, a field or method of a handle, or
    /// a hook, which the release does not run yet.
    Planned(String),
}

impl fmt::Display for ScriptProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScriptProblem::Unreferenced => f.write_str("no data names it"),
            ScriptProblem::Missing => f.write_str("named, but not held"),
            ScriptProblem::Compile(error) => write!(f, "{error}"),
            ScriptProblem::UnknownHook(function) => {
                write!(f, "{function} is no hook of the script's roles")
            }
            ScriptProblem::UnknownCtx(name) => {
                write!(f, "ctx.{name} is not the script API's for this script")
            }
            ScriptProblem::UnknownMember(name) => {
                write!(f, ".{name} is no member the script API has")
            }
            ScriptProblem::UnknownState(name) => {
                write!(f, ".state.{name} is a field no state of the match declares")
            }
            ScriptProblem::EnumString { call, takes } => {
                write!(f, "{call} takes a member of {takes}, not a string")
            }
            ScriptProblem::UnknownEnumMember { path, of } => {
                write!(f, "{path} is no member of {of}, nor its function")
            }
            ScriptProblem::FunctionPointer => {
                f.write_str("makes a function pointer: a closure, an anonymous function or Fn")
            }
            ScriptProblem::CtxMisuse(misuse) => write!(f, "{misuse}"),
            ScriptProblem::Planned(name) => {
                write!(f, "{name} is planned, and the release does not run it yet")
            }
        }
    }
}
