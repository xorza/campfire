use std::error::Error;
use std::fmt;

use campfire_math::Num;
use campfire_script::CapabilityRefusal;
use campfire_sim::Capability;

campfire_common::enum_all! {
    /// Why the production capability refused a script call: a script gave it a value it does not
    /// take.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ProductionRefusal {
        /// A count of supply in a mode that counts none.
        NoSupply,
    }
    const ALL;
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
