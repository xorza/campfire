use campfire_capabilities::{PoolId, ResourceId, TrackId};
use thiserror::Error;

/// What a mode declares more of than a match holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum Limit {
    /// Tags, its unit types' together.
    #[error("more tags than a match holds")]
    Tags,
    /// Unit types, its avatars' and its dependencies' delivery types among them.
    #[error("more unit types than a match holds")]
    UnitTypes,
    /// Tracks, more than a unit holds.
    #[error("more than {} tracks", TrackId::LIMIT)]
    Tracks,
    #[error("more damage kinds than a match tells apart")]
    DamageKinds,
    #[error("more than {} pools", PoolId::LIMIT)]
    Pools,
    #[error("more than {} player resources", ResourceId::LIMIT)]
    Resources,
    /// Packages, the mode's and those it depends on, more than a package index counts.
    #[error("more packages than a package index counts")]
    Packages,
}
