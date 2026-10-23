use std::error::Error;
use std::fmt;

use campfire_common::{DeclaredName, EnumAll};
use campfire_math::Num;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use thiserror::Error;

use crate::spawn_spec::SpawnSpec;

/// Why a unit type breaks a rule of production. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProductionDataError {
    /// A supply of a unit type in a mode that counts none.
    #[error("a supply in a mode that counts none")]
    SupplyUncounted,
    /// A train in the slots of a unit type with no queue to train in.
    #[error("train {0} sits on a unit type with no queue")]
    NoQueue(DeclaredName),
    /// A train that aims at something, where it takes no target.
    #[error("a train takes no target")]
    TrainAims,
    /// A train that makes a projectile or an area type, whose units only actions deliver.
    #[error("a train makes a projectile or an area type")]
    Trained,
    /// A train that makes a unit type that does not walk out of its producer.
    #[error("a train makes a unit type that walks")]
    TrainStands,
    /// A build that aims at something but a point.
    #[error("a build aims at a point")]
    BuildAims,
    /// A gather that aims at something but a unit its filter selects.
    #[error("a gather aims at a unit its filter selects")]
    GatherAims,
    /// A gather whose `unit` takes more than its whole trip's `take`.
    #[error("a gather's unit passes its take")]
    UnitPastTake,
    /// A free unit or a kept spawn of a projectile or an area type, whose units only actions
    /// deliver.
    #[error("a free unit or a kept spawn is of a projectile or an area type")]
    LeaverDelivered,
    /// A free unit or a kept spawn of a type that does not walk out of its exit.
    #[error("a free unit's or a kept spawn's type does not walk")]
    LeaverStands,
    /// A kept spawn of a type of no life, which would never die to come back.
    #[error("a kept spawn's type has no life")]
    SpawnUnliving,
    /// A type's `spawns` of no unit type, or of more than a spawner's turn counts.
    #[error("a type's `spawns` take 1 to {most} unit types, not {0}", most = SpawnSpec::MOST_TYPES)]
    SpawnTypes(usize),
    /// A build whose unit type has no body, or a circle of no radius.
    #[error("its building has no body")]
    NoBody,
    /// A build whose site starts at no life: its `start_life` is 0, or missing for a building
    /// with the life pool.
    #[error("its building's site starts at no life")]
    StartLife,
}

/// Why the production capability refused a script call: a script gave it a value it does not
/// take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum ProductionRefusal {
    /// A count of supply in a mode that counts none.
    NoSupply,
}

impl CapabilityRefusal for ProductionRefusal {
    const CAPABILITY: Capability = Capability::Production;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NoSupply => "the mode counts no supply",
        }
    }
}

/// A producer's point, `[x, z]`, with a coordinate past `Vec3::BOUND`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointPastBound(pub [Num; 2]);

impl fmt::Display for PointPastBound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [x, z] = self.0;
        write!(f, "[{x}, {z}] lies past 2²⁰ m of its producer")
    }
}

impl Error for PointPastBound {}
