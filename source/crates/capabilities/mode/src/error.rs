use campfire_common::EnumAll;
use campfire_geometry::PolygonError;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use campfire_units::GroundError;
use campfire_values::DeclaredName;
use thiserror::Error;

/// A list of the mode's names, as its data gives it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeList {
    Pools,
    Resources,
    DamageKinds,
    Layers,
}

/// Why a list of the mode's names does not load. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModeNamesError {
    /// More names than the ids of their kind count.
    #[error("more {0:?} than a match holds")]
    TooMany(ModeList),
    /// A name twice.
    #[error("{list:?}: {name} twice")]
    Repeated { list: ModeList, name: DeclaredName },
}

/// Why the mode's teams, relations or map do not load: they name what the mode does not have.
/// Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModeError {
    /// More teams than a team index counts.
    #[error("more teams than a team index counts")]
    TooManyTeams,
    /// More teams than a map with a vision grid holds.
    #[error("more teams than a map with a vision grid holds")]
    TooManyVisionTeams,
    /// A relation of a team to itself, or of a pair the relations name before.
    #[error("a relation of \"{0}\" and \"{1}\" again, or of a team to itself")]
    RepeatedRelation(DeclaredName, DeclaredName),
    #[error("\"{0}\" names two teams")]
    RepeatedTeam(DeclaredName),
    #[error("\"{0}\" names two paths")]
    RepeatedPath(DeclaredName),
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
    /// A path has no waypoint.
    #[error("path \"{0}\" has no waypoint")]
    EmptyPath(DeclaredName),
    /// A placed unit of a spatial map gives a height, which its point holds.
    #[error("a placed unit of a spatial map gives a height")]
    SpatialHeight,
    /// A point of the map is outside its bounds.
    #[error("a map point is outside the map's bounds")]
    OutOfBounds,
    /// The grid's cell is not positive or beyond the world's bound, or it makes more than 2²²
    /// cells.
    #[error("the grid needs a positive cell within the world's bound and at most 2²² cells")]
    Grid,
    /// A wall names a layer the mode does not declare.
    #[error("no layer \"{0}\"")]
    UnknownLayer(DeclaredName),
    /// The points of wall `wall`, counted from 0, make no simple polygon.
    #[error("wall {wall}")]
    Wall {
        wall: usize,
        #[source]
        problem: PolygonError,
    },
    /// The points of brush `brush`, counted from 0, make no simple polygon.
    #[error("brush {brush}")]
    Brush {
        brush: usize,
        #[source]
        problem: PolygonError,
    },
    /// The map's `[ground]` reads a height grid the map does not have.
    #[error("the map's [ground] reads a height grid it does not have")]
    NoHeights,
    #[error("the map's ground")]
    Ground(#[from] GroundError),
}

/// Why the mode capability refused a script call: a script gave it a value it does not take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum ModeRefusal {
    /// A count of values other than the choice's.
    ChoiceCount,
    /// A value of a unique choice another player chose.
    ChoiceTaken,
    /// A second end of a match.
    Ended,
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
            Self::Ended => "the match has ended",
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
