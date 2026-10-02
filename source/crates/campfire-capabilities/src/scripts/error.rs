use std::error::Error;
use std::fmt;

use campfire_math::Num;
use campfire_script::rhai::{EvalAltResult, INT};
use campfire_script::{Raised, ScriptError};

use crate::scripts::script_limits::ScriptLimits;

/// What a registered script function returns: its value, or the error that fails the call.
pub(crate) type Checked<T> = Result<T, Box<EvalAltResult>>;

/// Why a script call failed. A failed call changes nothing.
#[derive(Debug, Clone)]
pub enum CallError {
    /// A param of the ability overflows at its rank.
    ParamOverflow,
    /// The script API refused a call.
    Api(ApiError),
    /// The script failed otherwise.
    Script(ScriptError),
}

/// Why the script API refused a call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiError {
    /// A param the ability or the unit type does not declare.
    UnknownParam,
    /// A filter that is not a relation, such as `enemies`, with its tags, if any, after a `:`.
    UnknownFilter,
    /// A filter's tag that no unit type of the match declares.
    UnknownTag,
    NegativeRadius,
    NegativeTime,
    /// A time too long to count in ticks.
    TimeTooLarge,
    /// A damage kind the mode does not declare.
    UnknownDamageKind,
    NegativeDamage,
    /// A track the mode does not declare.
    UnknownTrack,
    /// Experience on a track the unit does not have.
    NoTrack,
    /// A reset of a unit with no spawn place to walk to.
    NoSpawnPlace,
    /// A projectile or an area of an action that delivers none of that kind.
    NoDelivery,
    NegativeXp,
    /// An integer beyond a `Num`, which reaches 2³⁹.
    IntegerBeyondNum,
    /// An attack field or order for a unit that has no attack, or an attack order on a target
    /// none of its weapons selects.
    NoAttack,
    /// A stat or level field of a unit that has no stats.
    NoStats,
    /// A stat the mode does not declare.
    UnknownStat,
    /// A hook at the depth of combat events no chain reaches.
    ChainTooDeep,
    /// A heal or restore of a negative amount.
    NegativeHeal,
    /// A call of a role the running script does not serve.
    NotForRole,
    /// A name that reads the mode, in a match with none.
    NoMode,
    /// A change from a pure hook's `ctx`, which only reads.
    PureCall,
    /// A `calc_damage` that returns no number.
    NotAnAmount,
    /// A modifier the calling package does not declare.
    UnknownModifier,
    /// A negative count of a modifier's stacks.
    NegativeStacks,
    /// A pool the unit does not have.
    NoPool,
    /// A pool the mode does not declare.
    UnknownPool,
    /// An AI order for another unit than the one that thinks.
    OtherUnit,
    /// An attack order on a unit that is not a living enemy.
    NotAnEnemy,
    /// A team the mode does not have.
    UnknownTeam,
    /// An end of a path other than `start` or `end`.
    UnknownPathEnd,
    /// A relation other than `hostile`, `neutral` or `friendly`.
    UnknownRelation,
    /// A relation of a team to itself, which is friendly.
    SelfRelation,
    /// A unit type the mode does not have.
    UnknownUnitType,
    /// A path the map does not have.
    UnknownPath,
    /// A point outside the map's bounds.
    OutOfBounds,
    /// A team with no one enemy team: `enemy_team` needs a mode of two teams.
    NoEnemyTeam,
    /// A second end of a match.
    Ended,
    /// A player slot the session does not have.
    UnknownPlayer,
    /// A choice the mode does not declare.
    UnknownChoice,
    /// A count of values other than the choice's.
    ChoiceCount,
    /// A value the choice does not offer.
    UnknownChoiceValue,
    /// A value chosen twice in one choice.
    RepeatedChoiceValue,
    /// A value of a unique choice another player chose.
    ChoiceTaken,
    /// A slot kind the mode does not declare.
    UnknownSlotKind,
    /// An action that is no loadout entry the mode depends on.
    UnknownAction,
    /// An action whose ranks are not those of the slot kind it goes in.
    SlotKindRanks,
    /// More slots than a unit holds.
    TooManySlots,
    /// A field of the mode's state it does not declare.
    UnknownState,
    /// A value not of its state field's type.
    WrongStateType,
    /// Timer data that is not `()`, a bool, an integer, a `Num`, a string, a unit, a list of
    /// units or a position.
    TimerData,
    /// A player resource the mode does not declare.
    UnknownResource,
    /// A player's resource past what an integer holds.
    ResourceOverflow,
    /// A unit to respawn that is alive.
    RespawnAlive,
    /// A unit to respawn whose type despawns when it dies.
    RespawnDespawns,
    /// A slot the unit has no ability in.
    NoAbilitySlot,
    /// An ability to learn that is at its last rank already.
    MaxRank,
    /// A projectile launched in the other form than its type flies: a homing type at a unit, a
    /// line type along a direction.
    OtherFlight,
}

impl CallError {
    /// A failed call's error: the API's own when it refused the call.
    pub(crate) fn from_script(error: ScriptError) -> CallError {
        let refused = match &error {
            ScriptError::Raised(raised) => raised.get::<ApiError>(),
            _ => None,
        };
        refused.map_or(CallError::Script(error), CallError::Api)
    }
}

impl ApiError {
    /// The error that fails the call with this refusal.
    pub(crate) fn fail(self) -> EvalAltResult {
        Raised::error(self)
    }

    /// A script's integer as a `Num`; one beyond a `Num` fails the call.
    pub(crate) fn num(value: INT) -> Checked<Num> {
        Num::from_int(value).ok_or_else(|| ApiError::IntegerBeyondNum.fail().into())
    }
}

impl fmt::Display for CallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CallError::ParamOverflow => f.write_str("a param overflows at the ability's rank"),
            CallError::Api(error) => write!(f, "the script API refused a call: {error}"),
            CallError::Script(error) => write!(f, "{error}"),
        }
    }
}

impl Error for CallError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            CallError::Api(error) => Some(error),
            CallError::Script(error) => Some(error),
            CallError::ParamOverflow => None,
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ApiError::UnknownParam => "param is not declared",
            ApiError::UnknownFilter => "filter is not a relation with its tags",
            ApiError::UnknownTag => "tag is not declared by any unit type",
            ApiError::NegativeRadius => "radius is negative",
            ApiError::NegativeTime => "time is negative",
            ApiError::TimeTooLarge => "time is too long to count in ticks",
            ApiError::UnknownDamageKind => "damage kind is not one the mode declares",
            ApiError::NegativeDamage => "damage is negative",
            ApiError::UnknownTrack => "no such track",
            ApiError::NoTrack => "the unit does not have the track",
            ApiError::NoSpawnPlace => "the unit has no spawn place",
            ApiError::NoDelivery => "the action delivers no unit of that kind",
            ApiError::NegativeXp => "experience is negative",
            ApiError::IntegerBeyondNum => "integer is beyond a Num",
            ApiError::NoAttack => "unit has no attack for the target",
            ApiError::NoStats => "unit has no stats",
            ApiError::UnknownStat => "stat the mode does not declare",
            ApiError::ChainTooDeep => {
                let depth = ScriptLimits::CHAIN_DEPTH;
                return write!(f, "a chain of combat events {depth} deep");
            }
            ApiError::NegativeHeal => "amount to heal or restore is negative",
            ApiError::NotForRole => "the call is not one of the script's role",
            ApiError::NoMode => "the match has no mode",
            ApiError::PureCall => "a pure hook changes nothing",
            ApiError::NotAnAmount => "calc_damage returns no number",
            ApiError::UnknownModifier => "modifier the package does not declare",
            ApiError::NegativeStacks => "a modifier's stacks are not negative",
            ApiError::NoPool => "unit has no such pool",
            ApiError::UnknownPool => "pool the mode does not declare",
            ApiError::OtherUnit => "an AI orders only the unit that thinks",
            ApiError::NotAnEnemy => "target is not a living enemy",
            ApiError::UnknownTeam => "team is not one of the mode's",
            ApiError::UnknownPathEnd => "a path's end is start or end",
            ApiError::UnknownRelation => "relation is not hostile, neutral or friendly",
            ApiError::SelfRelation => "a team's relation to itself is friendly",
            ApiError::UnknownUnitType => "unit type is not one of the mode's",
            ApiError::UnknownPath => "path is not one of the map's",
            ApiError::OutOfBounds => "point is outside the map's bounds",
            ApiError::NoEnemyTeam => "team has no one enemy team",
            ApiError::Ended => "the match has ended",
            ApiError::UnknownPlayer => "player is not in the session",
            ApiError::UnknownChoice => "choice is not one the mode declares",
            ApiError::ChoiceCount => "count of values is not the choice's",
            ApiError::UnknownChoiceValue => "value is not one the choice offers",
            ApiError::RepeatedChoiceValue => "value is chosen twice",
            ApiError::ChoiceTaken => "value is another player's choice",
            ApiError::UnknownSlotKind => "slot kind is not one the mode declares",
            ApiError::UnknownAction => "action is not a loadout entry the mode depends on",
            ApiError::SlotKindRanks => "action's ranks are not its slot kind's",
            ApiError::TooManySlots => "a unit holds at most 256 slots",
            ApiError::UnknownState => "state field is not declared",
            ApiError::WrongStateType => "value is not of the state field's type",
            ApiError::TimerData => "timer data is not a value state can hold",
            ApiError::UnknownResource => "player resource is not one the mode declares",
            ApiError::ResourceOverflow => "player resource overflows",
            ApiError::RespawnAlive => "unit to respawn is alive",
            ApiError::RespawnDespawns => "unit to respawn despawns when it dies",
            ApiError::NoAbilitySlot => "unit has no ability in that slot",
            ApiError::MaxRank => "ability is at its last rank",
            ApiError::OtherFlight => {
                "a homing projectile flies at a unit, and a line projectile along a direction"
            }
        })
    }
}

impl Error for ApiError {}

#[cfg(test)]
pub(crate) mod internals {
    use campfire_script::{NumError, ScriptError};

    use crate::scripts::error::{ApiError, CallError};

    /// What a failed call's error is, without what Rhai reported, so a test compares it: of what
    /// a script raised, the `NumError`, none for another value.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum FailureKind {
        ParamOverflow,
        Api(ApiError),
        Compile,
        CallLimit,
        TickBudget,
        Raised(Option<NumError>),
        Runtime,
    }

    impl CallError {
        pub(crate) fn kind(&self) -> FailureKind {
            match self {
                CallError::ParamOverflow => FailureKind::ParamOverflow,
                CallError::Api(api) => FailureKind::Api(*api),
                CallError::Script(ScriptError::Compile(_)) => FailureKind::Compile,
                CallError::Script(ScriptError::CallLimit) => FailureKind::CallLimit,
                CallError::Script(ScriptError::TickBudget) => FailureKind::TickBudget,
                CallError::Script(ScriptError::Raised(raised)) => {
                    FailureKind::Raised(raised.get::<NumError>())
                }
                CallError::Script(ScriptError::Runtime(_)) => FailureKind::Runtime,
            }
        }
    }
}
