use campfire_script::{ApiVersion, CtxMisuse, ScriptError};
use campfire_values::EngineEnum;
use thiserror::Error;

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
    /// It calls `name` with a count of arguments, `args`, that no form of it takes.
    #[error("{name} takes no form of {args} arguments")]
    Arity { name: String, args: usize },
    /// It reads a field or calls a method no handle, no built-in and none of its own functions
    /// or object maps has.
    #[error(".{0} is no member the script API has")]
    UnknownMember(String),
    /// It is a rules script of the mode's list, and uses `ctx.{0}`, which only the mode's own
    /// script may (03-game-scripting.md, Rules scripts in order).
    #[error("ctx.{0} is the mode's own script's alone, and this is a rules script of its list")]
    OwnScriptOnly(String),
    /// It defines the pure hook `{0}`, which a script before it in the mode's list defines
    /// (03-game-scripting.md, A pure hook in one script).
    #[error("{0} is a pure hook an earlier script of the mode's list defines")]
    PureHookTwice(String),
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
    /// It holds statements outside its functions, which no call runs (06-script-api.md, A script
    /// is its functions).
    #[error("holds statements outside its functions, which no call runs")]
    TopLevel,
    /// It uses `ctx` other than the script API's convention allows (06-script-api.md, `ctx` stays
    /// visible), so the load checks cannot see every use of it.
    #[error(transparent)]
    CtxMisuse(CtxMisuse),
    /// It reads, in a `switch`, the name `.{0}` returns, whose cases the load cannot check
    /// (06-script-api.md, A returned name is checked where a script compares it).
    #[error("switches on .{0}, a name whose cases the load cannot check: compare it with `==`")]
    NameSwitch(String),
    /// It compares with a literal the name `.{0}` returns, which holds no package scope, so the
    /// literal could name another package's (06-script-api.md, A scoped name compares through a
    /// method).
    #[error(".{0} gives a name of no package: compare through the handle's method")]
    ScopedCompare(String),
    /// It uses or defines `name`, which came in the package API version `since`, past the one
    /// its package targets.
    #[error("{name} came in the package API {since}, past the one the package targets")]
    Later { name: String, since: ApiVersion },
    /// It uses or defines a name the script API plans, a `ctx` name, a field or method of a handle,
    /// or a hook, which the release does not run yet.
    #[error("{0} is planned, and the release does not run it yet")]
    Planned(String),
}
