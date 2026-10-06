use rhai::{Dynamic, EvalAltResult, ParseError, Position, Variant};
use thiserror::Error;

/// Why a script did not compile or a call failed. Scripts come from packages, so each is an
/// expected failure; a failed call changes nothing.
#[derive(Debug, Clone, Error)]
pub enum ScriptError {
    /// The source does not compile.
    #[error("script does not compile: {0}")]
    Compile(ParseError),
    /// The call ran past the operation limit of one call.
    #[error("script call over its operation limit")]
    CallLimit,
    /// The call ran past what its budget had left, or found it spent.
    #[error("script call over the tick's operation budget")]
    TickBudget,
    /// A value raised and not caught: an API's error, such as a `NumError`, or a script's own
    /// `throw`.
    #[error("script call raised {:?}", .0.0)]
    Raised(Raised),
    /// Any other failure Rhai reports, such as an integer overflow, a limit of depth or size, a
    /// missing function or a wrong type: its message, with its position.
    #[error("script call failed: {0}")]
    Runtime(String),
}

impl ScriptError {
    pub(crate) fn from_eval(error: &EvalAltResult) -> ScriptError {
        match error.unwrap_inner() {
            EvalAltResult::ErrorTooManyOperations(_) => ScriptError::CallLimit,
            EvalAltResult::ErrorTerminated(..) => ScriptError::TickBudget,
            EvalAltResult::ErrorRuntime(value, _) => ScriptError::Raised(Raised(value.clone())),
            _ => ScriptError::Runtime(error.to_string()),
        }
    }
}

/// A value a call raised. An API raises its own error type with `error`, which its caller takes
/// back with `get`.
#[derive(Debug, Clone)]
pub struct Raised(Dynamic);

impl Raised {
    /// The error a registered function returns to raise `value`.
    pub fn error<T: Variant + Clone>(value: T) -> EvalAltResult {
        EvalAltResult::ErrorRuntime(Dynamic::from(value), Position::NONE)
    }

    /// The raised value as a `T`, when it is one.
    pub fn get<T: Clone + 'static>(&self) -> Option<T> {
        self.0.clone().try_cast::<T>()
    }
}

/// Why `Num` arithmetic in a script failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumError {
    /// The result is beyond a `Num`, or divides by zero.
    Overflow,
    /// An integer beyond a `Num`, which reaches 2³⁹.
    IntegerBeyondNum,
    /// `clamp` with a low bound above the high one.
    ClampBounds,
}
