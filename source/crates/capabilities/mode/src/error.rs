use campfire_common::{DeclaredName, EnumAll, Seat};
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
    /// A player controls its avatar in a mode of no avatar.
    #[error("avatar control in a mode of no avatar")]
    AvatarControlWithoutAvatars,
    /// A field of the mode's state goes to an owner or a team, which the mode's state has none
    /// of.
    #[error("a field of the mode's state goes to an owner or a team")]
    StateAudience,
    /// An option and a param of one name, which `ctx.params` reads as one (03-game-scripting.md, A
    /// mode declares its options).
    #[error("the mode declares a param of the option's name")]
    OptionParam,
    /// A choice of loadout entries names no slot kind, or a choice of avatars names one.
    #[error("a choice of loadout entries fills a slot kind, one of avatars none")]
    ChoiceSlot,
    /// A choice of values lists none, 255 or more, or one twice, or takes more than one; or another
    /// choice lists values.
    #[error(
        "a choice of values lists its values, each once and fewer than 255, and takes one; another \
         lists none"
    )]
    ChoiceValues,
    /// Two choices of loadout entries fill slot kinds of other ranks.
    #[error("choices of loadout entries fill slot kinds of other ranks")]
    LoadoutRanks,
}

/// Why the picks a session's terms carry are none of the mode's options (08-sessions.md, The
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

/// Why a host's picks are none of the mode's: of an option, or of a seat's choice.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum HostPickError {
    #[error(transparent)]
    Option(#[from] PickError),
    #[error(transparent)]
    Seat(#[from] SeatPickError),
}

/// Why seat picks are none of the mode's: the terms carry another count of them, or a place past
/// a choice's values, or a host names a seat, a choice or a value the mode lacks, picks one seat's
/// choice twice, or gives two seats one value of a unique choice.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SeatPickError {
    /// Picks of another count than the seats times the mode's choices of values.
    #[error("{given} seat picks, where the seats and the mode's choices of values take {expected}")]
    Count { given: usize, expected: usize },
    /// A pick past its choice's values.
    #[error("the pick {place} of the choice {choice} of seat {}, which has no value there", seat.get())]
    Place {
        seat: Seat,
        choice: DeclaredName,
        place: u8,
    },
    /// A pick of a seat past the session's.
    #[error("seat {} is none of the session's", _0.get())]
    Seat(Seat),
    /// A pick of a choice the mode does not declare, or one of avatars or loadout entries.
    #[error("the mode has no choice of values {0}")]
    Unknown(DeclaredName),
    /// A pick of a value its choice does not list.
    #[error("the choice {choice} lists no value {value}")]
    NoValue {
        choice: DeclaredName,
        value: DeclaredName,
    },
    /// Two picks of one seat's choice.
    #[error("the choice {choice} of seat {} is picked twice", seat.get())]
    Twice { seat: Seat, choice: DeclaredName },
    /// Two seats' picks of one value of a unique choice.
    #[error("two seats pick the value {place} of the unique choice {choice}")]
    Taken { choice: DeclaredName, place: u8 },
}

/// Why a host's `--pick` is no seat pick: it is not `seat:choice=value`, or its seat, its choice
/// or its value does not read.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SeatPickTextError {
    #[error("a seat's choice is picked as seat:choice=value")]
    Form,
    #[error("{0:?} is no seat")]
    Seat(String),
    #[error("{0:?} is no choice's name")]
    Choice(String),
    #[error("{0:?} is no value's name")]
    Value(String),
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

/// Why the mode's kill bounty does not load: a fault of its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum BountyError {
    /// A unit type gives a `[bounty]` in a mode that gives none.
    #[error("a unit type's bounty in a mode of no [bounty]")]
    NoModeBounty,
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
    /// A resource amount to add or take below 0.
    NegativeAmount,
    /// A slot the unit has no ability in.
    NoAbilitySlot,
    /// A body with no room where it would spawn: past the bounds, or over a wall, a body that
    /// stands or one the call spawns.
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
    /// A resource to take of which the player holds less.
    ResourceShort,
    /// A bounty share below 0.
    NegativeShare,
    /// A bounty share in a mode of no `[bounty]`.
    NoBounty,
    /// A search for room of a gap not above 0, or of steps below 0 or past `GridSearch::MAX_STEPS`.
    RoomSearch,
    /// A unit to respawn that is alive.
    RespawnAlive,
    /// A unit to respawn whose type despawns when it dies.
    RespawnDespawns,
    /// A call of a rules script to what only the mode's own script does: end the match, or record
    /// a choice.
    RulesScript,
    /// A relation of a team to itself, which is friendly.
    SelfRelation,
    /// An action whose ranks are not those of the slot kind it goes in.
    SlotKindRanks,
    /// Timer data that is not `()`, a bool, an integer, a `Num`, a string, a unit, a position, or
    /// a list of units, bools, integers, `Num`s or strings.
    TimerData,
    /// More slots than a unit holds.
    TooManySlots,
    /// A value the choice does not offer.
    UnknownChoiceValue,
}

impl CapabilityRefusal for ModeRefusal {
    const CAPABILITY: Capability = Capability::Mode;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::ChoiceCount => "count of values is not the choice's",
            Self::ChoiceTaken => "value is another player's choice",
            Self::MaxRank => "ability is at its last rank",
            Self::NegativeAmount => "resource amount is below 0",
            Self::NoAbilitySlot => "unit has no ability in that slot",
            Self::NoRoom => {
                "the body has no room there: past the bounds, or over a wall, a body that stands \
                 or one the call spawns"
            }
            Self::NotAnAmount => "calc_damage or calc_heal returns no number",
            Self::OutOfBounds => "point is outside the map's bounds",
            Self::PlayerOffTeam => "player is not on the unit's team",
            Self::RepeatedChoiceValue => "value is chosen twice",
            Self::ResourceOverflow => "player resource overflows",
            Self::ResourceShort => "player holds less of the resource",
            Self::NegativeShare => "bounty share is below 0",
            Self::NoBounty => "the mode gives no [bounty]",
            Self::RoomSearch => {
                "a search for room takes a gap above 0 and a range of 0 to 128 gaps"
            }
            Self::RespawnAlive => "unit to respawn is alive",
            Self::RespawnDespawns => "unit to respawn despawns when it dies",
            Self::RulesScript => "only the mode's own script ends the match or records a choice",
            Self::SelfRelation => "a team's relation to itself is friendly",
            Self::SlotKindRanks => "action's ranks are not its slot kind's",
            Self::TimerData => "timer data is not a value state can hold",
            Self::TooManySlots => "a unit holds at most 256 slots",
            Self::UnknownChoiceValue => "value is not one the choice offers",
        }
    }
}

/// Why values and a default make no option. Data is untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum OptionDataError {
    #[error("an option holds from 1 to 256 values")]
    Count,
    #[error("an option holds each value once")]
    Repeated,
    #[error("an option's default is one of its values")]
    NoDefault,
}
