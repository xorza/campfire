use std::fmt;

use campfire_capabilities::{Pools, ResourceId, TrackId};

/// What a mode declares more of than a match holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Limit {
    /// Tags, its unit types' together.
    Tags,
    /// Unit types, its avatars' and its dependencies' delivery types among them.
    UnitTypes,
    /// Tracks, more than a unit holds.
    Tracks,
    DamageKinds,
    Pools,
    Resources,
    /// Packages, the mode's and those it depends on, more than a package index counts.
    Packages,
}

impl fmt::Display for Limit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Limit::Tags => f.write_str("more tags than a match holds"),
            Limit::UnitTypes => f.write_str("more unit types than a match holds"),
            Limit::Tracks => write!(f, "more than {} tracks", TrackId::LIMIT),
            Limit::DamageKinds => f.write_str("more damage kinds than a match tells apart"),
            Limit::Pools => write!(f, "more than {} pools", Pools::LIMIT),
            Limit::Resources => write!(f, "more than {} player resources", ResourceId::LIMIT),
            Limit::Packages => f.write_str("more packages than a package index counts"),
        }
    }
}
