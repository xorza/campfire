use thiserror::Error;

/// Why an `airfield` breaks a rule of airfields. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AirfieldError {
    /// Its places are not as many as its type's production names.
    #[error("its airfield's places are not its production's")]
    Places,
    /// A place names a runway it does not have.
    #[error("a place of its airfield names a runway it does not have")]
    Runway,
    /// A type of a sortie has no flight, so nothing flies it.
    #[error("its sortie's type does not fly")]
    SortieFlight,
    /// It heals no life a second.
    #[error("its airfield heals no life")]
    Heal,
    /// Its sortie's stranded loss is no share of a life above 0 to 1 a second.
    #[error("its sortie's stranded loss lies above 0 to its whole life a second")]
    StrandedShare,
}
