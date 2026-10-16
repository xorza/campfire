use campfire_common::EnumAll;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use campfire_values::{NameId, TrackId};
use thiserror::Error;

/// Why the mode's tracks do not load. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TrackError {
    /// More tracks than a unit holds.
    #[error("more than {} tracks", TrackId::LIMIT)]
    TooMany,
    /// More than one track is the `level` track.
    #[error("more than one `level` track")]
    LevelTracks,
}

/// Why the progression capability refused a script call: a script gave it a value it does not
/// take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum ProgressionRefusal {
    /// A negative amount of experience.
    NegativeXp,
    /// Experience on a track the unit does not have.
    NoTrack,
}

impl CapabilityRefusal for ProgressionRefusal {
    const OWNER: Capability = Capability::Progression;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NegativeXp => "experience is negative",
            Self::NoTrack => "the unit does not have the track",
        }
    }
}
