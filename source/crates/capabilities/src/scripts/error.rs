use std::error::Error;
use std::fmt;

use campfire_math::Num;
use campfire_script::rhai::{EvalAltResult, INT};
use campfire_script::{Raised, ScriptError};

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
    /// A filter whose relation is not `enemies`, `allies` or `all`.
    UnknownFilter,
    /// A filter's tag that no unit type of the match declares.
    UnknownTag,
    NegativeRadius,
    NegativeTime,
    /// A time too long to count in ticks.
    TimeTooLarge,
    /// A damage kind that is not `physical`, `magic` or `true`.
    UnknownDamageKind,
    NegativeDamage,
    /// An integer beyond a `Num`, which reaches 2³⁹.
    IntegerBeyondNum,
    /// An attack field or order for a unit that has no attack.
    NoAttack,
    /// An AI order for another unit than the one that thinks.
    OtherUnit,
    /// An attack order on a unit that is not a living enemy.
    NotAnEnemy,
    /// A team the mode does not have.
    UnknownTeam,
    /// A unit type the mode does not have.
    UnknownUnitType,
    /// A lane the map does not have.
    UnknownLane,
    /// A team whose units walk no lane: only the first two teams walk each lane, from their own
    /// end.
    NoLaneEnd,
    /// A team with no one enemy team: `enemy_team` needs a mode of two teams.
    NoEnemyTeam,
    /// A player slot the session does not have.
    UnknownPlayer,
    /// A hero the mode does not depend on.
    UnknownHero,
    /// A hero another player chose.
    HeroTaken,
    /// A spell the mode does not depend on.
    UnknownSpell,
    /// A spell chosen twice.
    RepeatedSpell,
    /// A field of the mode's state it does not declare.
    UnknownState,
    /// A value not of its state field's type.
    WrongStateType,
    /// Timer data that is not `()`, a bool, an integer, a `Num`, a string, a unit, a list of
    /// units or a position.
    TimerData,
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
            ApiError::UnknownFilter => "filter is not enemies, allies or all",
            ApiError::UnknownTag => "tag is not declared by any unit type",
            ApiError::NegativeRadius => "radius is negative",
            ApiError::NegativeTime => "time is negative",
            ApiError::TimeTooLarge => "time is too long to count in ticks",
            ApiError::UnknownDamageKind => "damage kind is not physical, magic or true",
            ApiError::NegativeDamage => "damage is negative",
            ApiError::IntegerBeyondNum => "integer is beyond a Num",
            ApiError::NoAttack => "unit has no attack",
            ApiError::OtherUnit => "an AI orders only the unit that thinks",
            ApiError::NotAnEnemy => "target is not a living enemy",
            ApiError::UnknownTeam => "team is not one of the mode's",
            ApiError::UnknownUnitType => "unit type is not one of the mode's",
            ApiError::UnknownLane => "lane is not one of the map's",
            ApiError::NoLaneEnd => "team has no end of the lanes",
            ApiError::NoEnemyTeam => "team has no one enemy team",
            ApiError::UnknownPlayer => "player is not in the session",
            ApiError::UnknownHero => "hero is not one the mode depends on",
            ApiError::HeroTaken => "hero is another player's choice",
            ApiError::UnknownSpell => "spell is not one the mode depends on",
            ApiError::RepeatedSpell => "spell is chosen twice",
            ApiError::UnknownState => "state field is not declared",
            ApiError::WrongStateType => "value is not of the state field's type",
            ApiError::TimerData => "timer data is not a value state can hold",
            ApiError::ResourceOverflow => "player resource overflows",
            ApiError::RespawnAlive => "unit to respawn is alive",
            ApiError::RespawnDespawns => "unit to respawn despawns when it dies",
            ApiError::NoAbilitySlot => "unit has no ability in that slot",
            ApiError::MaxRank => "ability is at its last rank",
        })
    }
}

impl Error for ApiError {}
