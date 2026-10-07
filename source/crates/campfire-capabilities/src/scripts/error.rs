use campfire_math::Num;
use campfire_script::rhai::{EvalAltResult, INT};
use campfire_script::{Raised, ScriptError};
use thiserror::Error;

use crate::scripts::script_limits::ScriptLimits;
use crate::values::engine_enum::EngineEnum;

/// What a registered script function returns: its value, or the error that fails the call.
pub(crate) type Checked<T> = Result<T, Box<EvalAltResult>>;

/// Why a script call failed. A failed call changes nothing.
#[derive(Debug, Clone, Error)]
pub enum CallError {
    /// The script API refused a call.
    #[error("the script API refused a call")]
    Api(#[source] ApiError),
    /// The script failed otherwise.
    #[error(transparent)]
    Script(ScriptError),
}

/// Why the script API refused a call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ApiError {
    /// A param the ability or the unit type does not declare.
    #[error("param is not declared")]
    UnknownParam,
    /// A filter that is not a relation, such as `enemies`, with its tags, if any, after a `:`.
    #[error("filter is not a relation with its tags")]
    UnknownFilter,
    /// A filter's tag that no unit type of the match declares.
    #[error("tag is not declared by any unit type")]
    UnknownTag,
    #[error("radius is negative")]
    NegativeRadius,
    #[error("time is negative")]
    NegativeTime,
    /// A time too long to count in ticks.
    #[error("time is too long to count in ticks")]
    TimeTooLarge,
    /// A damage kind the mode does not declare.
    #[error("damage kind is not one the mode declares")]
    UnknownDamageKind,
    #[error("damage is negative")]
    NegativeDamage,
    /// A track the mode does not declare.
    #[error("no such track")]
    UnknownTrack,
    /// Experience on a track the unit does not have.
    #[error("the unit does not have the track")]
    NoTrack,
    /// A reset of a unit with no spawn place to walk to.
    #[error("the unit has no spawn place")]
    NoSpawnPlace,
    /// A projectile or an area of an action that delivers none of that kind.
    #[error("the action delivers no unit of that kind")]
    NoDelivery,
    #[error("experience is negative")]
    NegativeXp,
    /// An integer beyond a `Num`, which reaches 2³⁹.
    #[error("integer is beyond a Num")]
    IntegerBeyondNum,
    /// An attack field or order for a unit that has no attack, or an attack order on a target
    /// none of its weapons selects.
    #[error("unit has no attack for the target")]
    NoAttack,
    /// A stat or level field of a unit that has no stats.
    #[error("unit has no stats")]
    NoStats,
    /// A stat the mode does not declare.
    #[error("stat the mode does not declare")]
    UnknownStat,
    /// A hook at the depth of combat events no chain reaches.
    #[error("a chain of combat events {} deep", ScriptLimits::CHAIN_DEPTH)]
    ChainTooDeep,
    /// A heal or restore of a negative amount.
    #[error("amount to heal or restore is negative")]
    NegativeHeal,
    /// A call of a role the running script does not serve.
    #[error("the call is not one of the script's role")]
    NotForRole,
    /// A name that reads the mode, in a match with none.
    #[error("the match has no mode")]
    NoMode,
    /// A change from a pure hook's `ctx`, which only reads.
    #[error("a pure hook changes nothing")]
    PureCall,
    /// A `calc_damage` that returns no number.
    #[error("calc_damage returns no number")]
    NotAnAmount,
    /// A modifier the calling package does not declare.
    #[error("modifier the package does not declare")]
    UnknownModifier,
    /// A modifier whose param, as the call's action at its rank or no action gives it, does not
    /// hold for `problem`.
    #[error("a param of the modifier, as the call applies it")]
    ModifierParam(#[source] ParamProblem),
    /// A negative count of a modifier's stacks.
    #[error("a modifier's stacks are not negative")]
    NegativeStacks,
    /// A pool the unit does not have.
    #[error("unit has no such pool")]
    NoPool,
    /// A pool the mode does not declare.
    #[error("pool the mode does not declare")]
    UnknownPool,
    /// An AI order for another unit than the one that thinks.
    #[error("an AI orders only the unit that thinks")]
    OtherUnit,
    /// An attack order on a unit that is not a living enemy.
    #[error("target is not a living enemy")]
    NotAnEnemy,
    /// A team the mode does not have.
    #[error("team is not one of the mode's")]
    UnknownTeam,
    /// A text that names no member of the engine enum, as its `named` reads it.
    #[error("text names no member of {0}")]
    UnknownMember(EngineEnum),
    /// A relation of a team to itself, which is friendly.
    #[error("a team's relation to itself is friendly")]
    SelfRelation,
    /// A unit type the mode does not have.
    #[error("unit type is not one of the mode's")]
    UnknownUnitType,
    /// A path the map does not have.
    #[error("path is not one of the map's")]
    UnknownPath,
    /// A point outside the map's bounds.
    #[error("point is outside the map's bounds")]
    OutOfBounds,
    /// A box with no room where it would spawn: past the bounds, or over a wall or a body that
    /// stands.
    #[error("the box has no room there: past the bounds, or over a wall or a body that stands")]
    NoRoom,
    /// A team with no one enemy team: `enemy_team` needs a mode of two teams.
    #[error("team has no one enemy team")]
    NoEnemyTeam,
    /// A second end of a match.
    #[error("the match has ended")]
    Ended,
    /// A player slot the session does not have.
    #[error("player is not in the session")]
    UnknownPlayer,
    /// A choice the mode does not declare.
    #[error("choice is not one the mode declares")]
    UnknownChoice,
    /// A count of values other than the choice's.
    #[error("count of values is not the choice's")]
    ChoiceCount,
    /// A value the choice does not offer.
    #[error("value is not one the choice offers")]
    UnknownChoiceValue,
    /// A value chosen twice in one choice.
    #[error("value is chosen twice")]
    RepeatedChoiceValue,
    /// A value of a unique choice another player chose.
    #[error("value is another player's choice")]
    ChoiceTaken,
    /// A slot kind the mode does not declare.
    #[error("slot kind is not one the mode declares")]
    UnknownSlotKind,
    /// An action that is no loadout entry the mode depends on.
    #[error("action is not a loadout entry the mode depends on")]
    UnknownAction,
    /// An action whose ranks are not those of the slot kind it goes in.
    #[error("action's ranks are not its slot kind's")]
    SlotKindRanks,
    /// More slots than a unit holds.
    #[error("a unit holds at most 256 slots")]
    TooManySlots,
    /// A field of the mode's state it does not declare.
    #[error("state field is not declared")]
    UnknownState,
    /// A value not of its state field's type.
    #[error("value is not of the state field's type")]
    WrongStateType,
    /// Timer data that is not `()`, a bool, an integer, a `Num`, a string, a unit, a list of
    /// units or a position.
    #[error("timer data is not a value state can hold")]
    TimerData,
    /// A player resource the mode does not declare.
    #[error("player resource is not one the mode declares")]
    UnknownResource,
    /// A player's resource past what an integer holds.
    #[error("player resource overflows")]
    ResourceOverflow,
    /// A unit to respawn that is alive.
    #[error("unit to respawn is alive")]
    RespawnAlive,
    /// A unit to respawn whose type despawns when it dies.
    #[error("unit to respawn despawns when it dies")]
    RespawnDespawns,
    /// A slot the unit has no ability in.
    #[error("unit has no ability in that slot")]
    NoAbilitySlot,
    /// An ability to learn that is at its last rank already.
    #[error("ability is at its last rank")]
    MaxRank,
    /// A projectile launched in the other form than its type flies: a homing type at a unit, a
    /// line type along a direction.
    #[error("a homing projectile flies at a unit, and a line projectile along a direction")]
    OtherFlight,
    /// A probability below 0 or above 1.
    #[error("probability is below 0 or above 1")]
    NotAProbability,
    /// A pick from an empty list.
    #[error("pick from an empty list")]
    EmptyPick,
    /// An ability the calling package does not declare.
    #[error("ability the package does not declare")]
    UnknownAbility,
    /// An ability the unit does not hold.
    #[error("the unit does not hold the ability")]
    NotHeld,
    /// A fraction below 0 or above 1.
    #[error("fraction is below 0 or above 1")]
    NotAFraction,
    /// An ability with no charges.
    #[error("the ability has no charges")]
    NoCharges,
    /// A value of an action's start in a call of an action that did not start, such as a weapon's.
    #[error("the call's action did not start")]
    NoStart,
    /// `ctx.charge` of an action that does not charge.
    #[error("the call's action does not charge")]
    NotCharged,
    /// A call that acts as its unit with no acting unit, as the mode's, or one that is gone.
    #[error("the call has no acting unit")]
    NoActingUnit,
    /// A time of 0 where something must last.
    #[error("time is 0")]
    ZeroTime,
    /// A forced move of a unit that does not walk.
    #[error("the unit does not walk")]
    NoWalker,
    /// A speed of no step a tick: 0, negative, or too small to move a bit.
    #[error("speed moves less than a bit a tick")]
    NotASpeed,
    /// A distance that is not more than 0, or beyond the world's bound.
    #[error("distance is not more than 0, or beyond the world's bound")]
    NotADistance,
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

#[cfg(test)]
pub(crate) mod internals {
    use campfire_script::{NumError, ScriptError};

    use crate::scripts::error::{ApiError, CallError};

    /// What a failed call's error is, without what Rhai reported, so a test compares it: of what
    /// a script raised, the `NumError`, none for another value.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum FailureKind {
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
