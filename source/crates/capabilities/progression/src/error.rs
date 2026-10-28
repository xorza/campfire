use campfire_common::{DeclaredName, EnumAll};
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use campfire_values::{NameId, PerkId, TrackId};
use thiserror::Error;

/// Why the mode's tracks do not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TrackError {
    /// The slot kind of that name gives its ranks levels, in a mode with no `level` track.
    #[error("slot kind {0}: its ranks' `levels` need a `level` track")]
    RankLevels(DeclaredName),
    /// More tracks than a unit holds.
    #[error("more than {} tracks", TrackId::LIMIT)]
    TooMany,
    /// More than one track is the `level` track.
    #[error("more than one `level` track")]
    LevelTracks,
    /// A unit type's own thresholds on `track`, `given` of them, where the track has `levels`.
    #[error("{given} thresholds of its own on track {track}, which has {levels}")]
    TypeLevels {
        track: DeclaredName,
        given: usize,
        levels: usize,
    },
    /// A unit type's start on the track of that name at `level`, where the track has `levels`.
    #[error("a start at level {level} on track {track}, which has {levels}")]
    StartLevel {
        track: DeclaredName,
        level: u32,
        levels: usize,
    },
    /// The track of that name holds `given` modifiers, where it has `levels` levels from 2.
    #[error("track {track}: {given} modifiers for its {levels} levels from 2")]
    Modifiers {
        track: DeclaredName,
        given: usize,
        levels: usize,
    },
    /// The track of that name holds `given` points, where it has `levels` levels.
    #[error("track {track}: {given} points for its {levels} levels")]
    Points {
        track: DeclaredName,
        given: usize,
        levels: usize,
    },
    /// The track of that name gives points, and is neither the `level` track, whose points alone
    /// a unit holds, nor a player's.
    #[error("track {0}: points on a track that is neither the `level` track nor a player's")]
    PointsOffLevel(DeclaredName),
    /// The track of that name is a player's and the `level` track, which a unit holds.
    #[error("track {0}: a player's track is no unit's `level`")]
    PlayerLevel(DeclaredName),
    /// A unit type lists the track of that name, which players hold.
    #[error("a unit type lists track {0}, which players hold")]
    PlayerTrack(DeclaredName),
}

/// Why the mode's perks do not load: a fault of the mode's data.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PerkError {
    /// More perks than a match holds.
    #[error("more than {} perks", PerkId::LIMIT)]
    TooMany,
    /// A perk that gives a `track` with no `level`, or a `level` with no `track`.
    #[error("a perk's `track` and `level` go together")]
    LevelTrack,
    /// A perk needs a level of the track of that name, which units hold.
    #[error("track {0} is no player's")]
    UnitTrack(DeclaredName),
    /// A perk needs `level` of a track of `levels`.
    #[error("level {level} of a track of {levels}")]
    Level { level: u32, levels: usize },
    /// A perk of a cost in a mode whose player tracks give no point to pay it.
    #[error("a cost in a mode whose player tracks give no point")]
    NoPoints,
}

/// Why the deaths' experience does not load: a fault of the mode's data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ExperienceError {
    /// A unit type gives `[experience]` in a mode that gives none.
    #[error("a unit type's experience in a mode of no [experience]")]
    NoModeExperience,
    /// A share of a unit's track to a player, or of a player's track to a unit.
    #[error("a share to a player of a unit's track, or to a unit of a player's")]
    ShareHolder,
    /// A list of experience by level in a mode of no `level` track, whose level the list reads.
    #[error("a unit type's experience by level in a mode of no `level` track")]
    NoLevelTrack,
    /// A list of experience of `given` values, for a track of `levels`.
    #[error("experience for {given} levels, of a track of {levels}")]
    Levels { given: usize, levels: usize },
}

/// Why the progression capability refused a script call: a script gave it a value it does not
/// take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum ProgressionRefusal {
    /// A negative amount of experience.
    NegativeXp,
    /// Experience on a track the unit does not have.
    NoTrack,
    /// A read of the points of a player of no track of points.
    NoPoints,
}

impl CapabilityRefusal for ProgressionRefusal {
    const CAPABILITY: Capability = Capability::Progression;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NegativeXp => "experience is negative",
            Self::NoTrack => "the unit does not have the track",
            Self::NoPoints => "the player has no track of points",
        }
    }
}
