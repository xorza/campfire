use campfire_sim::Capability;
use campfire_values::EngineEnum;
use campfire_values::NameKind;
use campfire_visibility::DataKind;
use rhai::EvalAltResult;
use thiserror::Error;

use crate::script_host::error::{NumError, Raised, ScriptError};
use crate::scripts::script_limits::ScriptLimits;

/// What a registered script function returns: its value, or the error that fails the call.
pub type Checked<T> = Result<T, Box<EvalAltResult>>;

/// Why a script call failed. A failed call changes nothing.
#[derive(Debug, Error)]
pub enum CallError {
    /// The script API refused a call.
    #[error("the script API refused a call")]
    Api(#[source] ApiError),
    /// The call found its budget spent, or spent it as it ran: what the caller answers in a later
    /// tick waits, or the call fails.
    #[error("the call's budget is spent")]
    BudgetSpent,
    /// The call ran past the operation limit of one call.
    #[error("script call over its operation limit")]
    CallLimit,
    /// A value raised and not caught, but the API's refusal: a `NumError`, or a script's own
    /// `throw`.
    #[error("script call raised {:?}", .0.0)]
    Raised(Raised),
    /// Any other failure Rhai reports, as `ScriptError::Runtime` gives it.
    #[error("script call failed")]
    Runtime(#[source] Box<EvalAltResult>),
}

/// Why the script API refused a call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ApiError {
    /// A name of the kind that the match does not declare where the call reads it: a modifier or
    /// an ability of the calling package, a unit type of the mode's scope, a param of the call or
    /// the unit's type, a filter that is not a relation with its tags.
    #[error("{0} is not one the match declares")]
    Unknown(NameKind),
    #[error("radius is negative")]
    NegativeRadius,
    /// A place with a coordinate past the world's bound, 2²⁰ m.
    #[error("point is past the world's bound")]
    PastBound,
    #[error("time is negative")]
    NegativeTime,
    /// A time too long to count in ticks.
    #[error("time is too long to count in ticks")]
    TimeTooLarge,
    /// A projectile or an area of an action that delivers none of that kind.
    #[error("the action delivers no unit of that kind")]
    NoDelivery,
    /// A hook at the depth of combat events no chain reaches.
    #[error("a chain of combat events {} deep", ScriptLimits::CHAIN_DEPTH)]
    ChainTooDeep,
    /// A call of a role the running script does not serve.
    #[error("the call is not one of the script's role")]
    NotForRole,
    /// A read of a unit's state of a kind its viewer's client does not receive, as a bot reads only
    /// what its player slot's client receives (03-game-scripting.md, A bot decides from what it
    /// sees, each think).
    #[error("its viewer receives no {} state of the unit", .0.name())]
    Unreceived(DataKind),
    /// A name that reads the mode, in a match with none.
    #[error("the match has no mode")]
    NoMode,
    /// A change from a pure hook's `ctx`, which only reads.
    #[error("a pure hook changes nothing")]
    PureCall,
    /// A text that names no member of the engine enum, as its `named` reads it.
    #[error("text names no member of {0}")]
    UnknownMember(EngineEnum),
    /// A read of a unit the call creates, which has no values until it spawns as the call
    /// applies.
    #[error("unit has not spawned yet")]
    Unspawned,
    /// A field of the mode's state it does not declare.
    #[error("state field is not declared")]
    UnknownState,
    /// A text that is no member of its enum state field.
    #[error("text is no member of the state field")]
    NotAMember,
    /// A value not of its state field's type.
    #[error("value is not of the state field's type")]
    WrongStateType,
    /// A probability below 0 or above 1.
    #[error("probability is below 0 or above 1")]
    NotAProbability,
    /// A pick from an empty list.
    #[error("pick from an empty list")]
    EmptyPick,
    /// A call that acts as its unit with no acting unit, as the mode's, or one that is gone.
    #[error("the call has no acting unit")]
    NoActingUnit,
    /// A time of 0 where something must last.
    #[error("time is 0")]
    ZeroTime,
    /// A list of units with a value that is no unit.
    #[error("value is no unit")]
    NotAUnit,
    /// A bot's order whose table reads as no directive, as an order script's `directive` reads.
    #[error("table is no directive")]
    NotADirective,
    /// A capability's own refusal, which its own enum of refusals gives.
    #[error("{0}")]
    Refused(Refusal),
}

/// A capability's own refusal of a script call: the capability, the code of the case among its
/// refusals, and the case's message. Two refusals of one capability and code are one case.
#[derive(Debug, Clone, Copy, Error)]
#[error("{message}")]
pub struct Refusal {
    capability: Capability,
    code: u8,
    message: &'static str,
}

impl Refusal {
    pub(crate) const fn new(capability: Capability, code: u8, message: &'static str) -> Self {
        Self {
            capability,
            code,
            message,
        }
    }
}

impl PartialEq for Refusal {
    fn eq(&self, other: &Self) -> bool {
        (self.capability, self.code) == (other.capability, other.code)
    }
}

impl Eq for Refusal {}

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

/// Why a param a modifier reads does not hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ParamProblem {
    /// The way gives no such param: an action that does not declare it, or no action at all.
    #[error("the way gives no such param")]
    Missing,
    /// A per-rank param, the modifier's own or its action's, has no value at the rank the way
    /// applies it at.
    #[error("no value at a rank the way applies it at")]
    Short,
    /// A value, at a rank, is past what a number holds.
    #[error("a value past what a number holds")]
    Overflow,
    /// A time reads a scaling param, whose value only its source knows as it applies.
    #[error("a time that reads a scaling param")]
    ScalingTime,
    /// A time, at a rank, is negative or too large to count in ticks.
    #[error("a time negative or too large to count in ticks")]
    Time,
}

impl CallError {
    /// A failed call's error: the API's own when it refused the call, and `BudgetSpent` for a
    /// spent budget.
    pub fn from_script(error: ScriptError) -> Self {
        match error {
            ScriptError::CallLimit => Self::CallLimit,
            ScriptError::TickBudget => Self::BudgetSpent,
            ScriptError::Raised(raised) => match raised.get::<ApiError>() {
                Some(api) => Self::Api(api),
                None => Self::Raised(raised),
            },
            ScriptError::Runtime(error) => Self::Runtime(error),
        }
    }

    /// What it is, its raised value read once.
    pub fn kind(&self) -> FailureKind {
        match self {
            Self::Api(api) => FailureKind::Api(*api),
            Self::BudgetSpent => FailureKind::BudgetSpent,
            Self::CallLimit => FailureKind::CallLimit,
            Self::Raised(raised) => FailureKind::Raised(raised.get::<NumError>()),
            Self::Runtime(_) => FailureKind::Runtime,
        }
    }
}

impl ApiError {
    /// The error that fails the call with this refusal.
    pub fn fail(self) -> EvalAltResult {
        Raised::error(self)
    }
}

/// What a failed call's error is, without what Rhai reported, so a host and a test tell it
/// apart: of what a script raised, the `NumError`, none for another value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureKind {
    Api(ApiError),
    CallLimit,
    BudgetSpent,
    Raised(Option<NumError>),
    Runtime,
}

#[cfg(test)]
mod tests {
    use rhai::{Dynamic, EvalAltResult, Position};

    use crate::script_host::error::{NumError, Raised, ScriptError};
    use crate::scripts::error::{ApiError, CallError, FailureKind};

    #[test]
    fn a_call_error_keeps_each_case_a_call_can_fail_by() {
        let kind = |error| CallError::from_script(error).kind();
        // A spent tick budget is `BudgetSpent`; an API's refusal is the API's, a `NumError` raised
        // is read back, and a script's own `throw` is a value of no error.
        assert_eq!(kind(ScriptError::TickBudget), FailureKind::BudgetSpent);
        assert_eq!(kind(ScriptError::CallLimit), FailureKind::CallLimit);
        let of = |value| ScriptError::Raised(Raised(value));
        assert_eq!(
            kind(of(Dynamic::from(ApiError::NoMode))),
            FailureKind::Api(ApiError::NoMode)
        );
        assert_eq!(
            kind(of(Dynamic::from(NumError::Overflow))),
            FailureKind::Raised(Some(NumError::Overflow))
        );
        assert_eq!(kind(of(Dynamic::from(1_i64))), FailureKind::Raised(None));
        let runtime = EvalAltResult::ErrorArithmetic(String::new(), Position::NONE);
        assert_eq!(
            kind(ScriptError::Runtime(runtime.into())),
            FailureKind::Runtime
        );
    }
}
