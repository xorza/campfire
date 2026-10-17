use std::error::Error;
use std::fmt;

use campfire_common::{DeclaredName, EnumAll};
use campfire_math::Num;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;
use thiserror::Error;

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
}

/// Why the production capability refused a script call: a script gave it a value it does not
/// take.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumAll)]
pub enum ProductionRefusal {
    /// A count of supply in a mode that counts none.
    NoSupply,
}

impl CapabilityRefusal for ProductionRefusal {
    const OWNER: Capability = Capability::Production;
    const CASES: &'static [Self] = &Self::ALL;

    fn message(self) -> &'static str {
        match self {
            Self::NoSupply => "the mode counts no supply",
        }
    }
}

/// A producer's point, `[x, z]`, with a coordinate past `Position::BOUND`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointPastBound(pub [Num; 2]);

impl fmt::Display for PointPastBound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [x, z] = self.0;
        write!(f, "[{x}, {z}] lies past 2²⁰ m of its producer")
    }
}

impl Error for PointPastBound {}
