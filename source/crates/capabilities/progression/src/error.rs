use campfire_core::TrackId;
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
