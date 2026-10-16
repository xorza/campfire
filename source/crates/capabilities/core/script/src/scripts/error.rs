use campfire_sim::Capability;
use campfire_values::EngineEnum;
use rhai::EvalAltResult;
use thiserror::Error;

use crate::script_host::error::{Raised, ScriptError};
use crate::scripts::name_kind::NameKind;
use crate::scripts::script_limits::ScriptLimits;

/// What a registered script function returns: its value, or the error that fails the call.
pub type Checked<T> = Result<T, Box<EvalAltResult>>;

/// Why a script call failed. A failed call changes nothing.
#[derive(Debug, Error)]
pub enum CallError {
    /// The script API refused a call.
    #[error("the script API refused a call")]
    Api(#[source] ApiError),
    /// The call found its pool spent, or spent it as it ran: what the caller answers in a later
    /// tick waits, or the call fails.
    #[error("the call's pool is spent")]
    PoolSpent,
    /// The script failed otherwise.
    #[error(transparent)]
    Script(ScriptError),
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
    /// An attack field or order for a unit that has no attack, or an attack order on a target
    /// none of its weapons selects.
    #[error("unit has no attack for the target")]
    NoAttack,
    /// A hook at the depth of combat events no chain reaches.
    #[error("a chain of combat events {} deep", ScriptLimits::CHAIN_DEPTH)]
    ChainTooDeep,
    /// A call of a role the running script does not serve.
    #[error("the call is not one of the script's role")]
    NotForRole,
    /// A name that reads the mode, in a match with none.
    #[error("the match has no mode")]
    NoMode,
    /// A change from a pure hook's `ctx`, which only reads.
    #[error("a pure hook changes nothing")]
    PureCall,
    /// A text that names no member of the engine enum, as its `named` reads it.
    #[error("text names no member of {0}")]
    UnknownMember(EngineEnum),
    /// A player slot the session does not have.
    #[error("player is not in the session")]
    UnknownPlayer,
    /// A field of the mode's state it does not declare.
    #[error("state field is not declared")]
    UnknownState,
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
    /// A capability's own refusal, which its own enum of refusals gives.
    #[error("{0}")]
    Refused(Refusal),
}

/// A capability's own refusal of a script call: the capability, the code of the case among its
/// refusals, and the case's message. Two refusals of one capability and code are one case.
#[derive(Debug, Clone, Copy, Error)]
#[error("{message}")]
pub struct Refusal {
    owner: Capability,
    code: u8,
    message: &'static str,
}

impl Refusal {
    pub(crate) const fn new(owner: Capability, code: u8, message: &'static str) -> Self {
        Self {
            owner,
            code,
            message,
        }
    }
}

impl PartialEq for Refusal {
    fn eq(&self, other: &Self) -> bool {
        (self.owner, self.code) == (other.owner, other.code)
    }
}

impl Eq for Refusal {}

/// A name that data gives of a kind the match does not declare: what a book refuses as it
/// resolves the name.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("no {of} {name:?}")]
pub struct UnknownName {
    pub of: NameKind,
    pub name: String,
}

impl UnknownName {
    /// The name `name`, which names no `of`.
    pub fn new(of: NameKind, name: &(impl ToString + ?Sized)) -> Self {
        Self {
            of,
            name: name.to_string(),
        }
    }
}

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
    /// A failed call's error: the API's own when it refused the call, and `PoolSpent` for a
    /// spent budget.
    pub fn from_script(error: ScriptError) -> Self {
        let refused = match &error {
            ScriptError::Raised(raised) => raised.get::<ApiError>(),
            ScriptError::TickBudget => return Self::PoolSpent,
            _ => None,
        };
        refused.map_or(Self::Script(error), CallError::Api)
    }
}

impl ApiError {
    /// The error that fails the call with this refusal.
    pub fn fail(self) -> EvalAltResult {
        Raised::error(self)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::script_host::error::{NumError, ScriptError};
    use crate::scripts::error::{ApiError, CallError};

    /// What a failed call's error is, without what Rhai reported, so a test compares it: of what
    /// a script raised, the `NumError`, none for another value.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FailureKind {
        Api(ApiError),
        Compile,
        CallLimit,
        PoolSpent,
        Raised(Option<NumError>),
        Runtime,
    }

    impl CallError {
        pub fn kind(&self) -> FailureKind {
            match self {
                Self::Api(api) => FailureKind::Api(*api),
                Self::Script(ScriptError::Compile(_)) => FailureKind::Compile,
                Self::Script(ScriptError::CallLimit) => FailureKind::CallLimit,
                Self::PoolSpent => FailureKind::PoolSpent,
                Self::Script(ScriptError::TickBudget) => {
                    unreachable!("a spent budget is a spent pool")
                }
                Self::Script(ScriptError::Raised(raised)) => {
                    FailureKind::Raised(raised.get::<NumError>())
                }
                Self::Script(ScriptError::Runtime(_)) => FailureKind::Runtime,
            }
        }
    }
}
