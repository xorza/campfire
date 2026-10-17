use campfire_common::{DeclaredName, EnumAll};
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use campfire_values::Scalar;
use thiserror::Error;

/// Why the mode's own data breaks a rule of the mode. Packages are untrusted, so each is an
/// expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModeDataError {
    /// A bot takes a leaver's place in a mode no player joins late.
    #[error("bot_takeover without late_join")]
    BotTakeoverWithoutLateJoin,
    /// A field of the mode's state syncs to an owner or a team, which the mode's state has none
    /// of.
    #[error("a field of the mode's state syncs to an owner or a team")]
    StateSync,
    /// An option and a param of one name, which `ctx.p` reads as one (03-game-scripting.md, A
    /// mode declares its options).
    #[error("the mode declares a param of the option's name")]
    OptionParam,
    /// A choice of loadout entries names no slot kind, or a choice of avatars names one.
    #[error("a choice of loadout entries fills a slot kind, one of avatars none")]
    ChoiceSlot,
    /// Two choices of loadout entries fill slot kinds of other ranks.
    #[error("choices of loadout entries fill slot kinds of other ranks")]
    LoadoutRanks,
}

/// Why the picks a session's terms carry are none of the mode's options (10-sessions.md, The
/// host picks, and the terms record the pick). Terms come from a server or a published log, so
/// each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PickError {
    /// Picks of another count than the mode's options.
    #[error("{given} picks of the mode's {options} options")]
    Count { given: usize, options: usize },
    /// A pick past its option's values.
    #[error("the pick {place} of the option {option}, which has no value there")]
    Place { option: DeclaredName, place: u8 },
    /// A host's pick of an option the mode does not declare.
    #[error("the mode has no option {0}")]
    Unknown(DeclaredName),
    /// A host's pick of a value its option lacks.
    #[error("the option {option} has no value {value}")]
    NoValue { option: DeclaredName, value: Scalar },
    /// A host's two picks of one option.
    #[error("the option {0} is picked twice")]
    Twice(DeclaredName),
}

/// Why a host's `--option` is no pick: it names no value, after `=`, or its name or its value
/// does not read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OptionValueError {
    #[error("an option is picked as name=value")]
    NoValue,
    #[error("{0:?} is no option's name")]
    Name(String),
    #[error("{0:?} is no number")]
    Value(String),
}

/// Why the mode's teams, relations or map do not load: they name what the mode does not have.
/// Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModeError {
    /// More teams than a team index counts.
    #[error("more teams than a team index counts")]
    TooManyTeams,
    /// A relation of a team to itself, or of a pair the relations name before.
    #[error("a relation of \"{0}\" and \"{1}\" again, or of a team to itself")]
    RepeatedRelation(DeclaredName, DeclaredName),
    #[error("\"{0}\" names two teams")]
    RepeatedTeam(DeclaredName),
    #[error("\"{0}\" names two markers")]
    RepeatedMarker(DeclaredName),
    #[error("no team \"{0}\"")]
    UnknownTeam(DeclaredName),
    #[error("no unit type \"{0}\"")]
    UnknownUnitType(DeclaredName),
    #[error("no path \"{0}\"")]
    UnknownPath(DeclaredName),
    /// A placed unit of the type walks from an end of no path.
    #[error("a placed \"{0}\" walks from an end of no path")]
    NoPathToWalk(DeclaredName),
    /// A point of the map has the shape of the other metric's points.
    #[error("a point of the map does not fit its metric")]
    PointShape,
    /// The marker's region is not a box within the map's bounds of its metric, or the marker has
    /// a point too.
    #[error("marker \"{0}\": the region is no box within the bounds, or has a point too")]
    Region(DeclaredName),
    /// A placed unit of a spatial map gives a height, which its point holds.
    #[error("a placed unit of a spatial map gives a height")]
    SpatialHeight,
    /// A point of the map is outside its bounds.
    #[error("a map point is outside the map's bounds")]
    OutOfBounds,
}

/// Why the mode capability refused a script call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum ModeRefusal {
    /// A count of values other than the choice's.
    ChoiceCount,
    /// A value of a unique choice another player chose.
    ChoiceTaken,
    /// An ability to learn that is at its last rank already.
    MaxRank,
    /// A slot the unit has no ability in.
    NoAbilitySlot,
    /// A team with no one enemy team: `enemy_team` needs a mode of two teams.
    NoEnemyTeam,
    /// A box with no room where it would spawn: past the bounds, or over a wall or a body that
    /// stands.
    NoRoom,
    /// A `calc_damage` or a `calc_heal` that returns no number.
    NotAnAmount,
    /// A point outside the map's bounds.
    OutOfBounds,
    /// A unit owned by a player of another team: a player's units play on its team, so its
    /// vision group is theirs.
    PlayerOffTeam,
    /// A value chosen twice in one choice.
    RepeatedChoiceValue,
    /// A player's resource past what an integer holds.
    ResourceOverflow,
    /// A unit to respawn that is alive.
    RespawnAlive,
    /// A unit to respawn whose type despawns when it dies.
    RespawnDespawns,
    /// A relation of a team to itself, which is friendly.
    SelfRelation,
    /// An action whose ranks are not those of the slot kind it goes in.
    SlotKindRanks,
    /// Timer data that is not `()`, a bool, an integer, a `Num`, a string, a unit, a list of
    /// units or a position.
    TimerData,
    /// More slots than a unit holds.
    TooManySlots,
    /// A value the choice does not offer.
    UnknownChoiceValue,
}

impl CapabilityRefusal for ModeRefusal {
    const OWNER: Capability = Capability::Mode;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::ChoiceCount => "count of values is not the choice's",
            Self::ChoiceTaken => "value is another player's choice",
            Self::MaxRank => "ability is at its last rank",
            Self::NoAbilitySlot => "unit has no ability in that slot",
            Self::NoEnemyTeam => "team has no one enemy team",
            Self::NoRoom => {
                "the box has no room there: past the bounds, or over a wall or a body that stands"
            }
            Self::NotAnAmount => "calc_damage or calc_heal returns no number",
            Self::OutOfBounds => "point is outside the map's bounds",
            Self::PlayerOffTeam => "player is not on the unit's team",
            Self::RepeatedChoiceValue => "value is chosen twice",
            Self::ResourceOverflow => "player resource overflows",
            Self::RespawnAlive => "unit to respawn is alive",
            Self::RespawnDespawns => "unit to respawn despawns when it dies",
            Self::SelfRelation => "a team's relation to itself is friendly",
            Self::SlotKindRanks => "action's ranks are not its slot kind's",
            Self::TimerData => "timer data is not a value state can hold",
            Self::TooManySlots => "a unit holds at most 256 slots",
            Self::UnknownChoiceValue => "value is not one the choice offers",
        }
    }
}
