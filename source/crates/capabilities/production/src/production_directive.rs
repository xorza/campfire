use campfire_players::CapabilityDirective;
use campfire_sim::Capability;
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

use crate::rally_target::RallyTarget;

/// What an order of `production` tells its units to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum ProductionDirective<R: OrderRefs = Ids> {
    /// Cancel the entry at `place` of a producer's train queue, the head 0.
    CancelTrain { place: u8 },
    /// Set where a producer's trained units go, or clear it with `None`.
    Rally { target: Option<RallyTarget<R>> },
    /// Cancel a site of the player's building.
    CancelBuild,
    /// Sell a building of the player's (04-capabilities/production.md, A building may be sold).
    Sell,
}

impl Copy for ProductionDirective {}

impl CapabilityDirective for ProductionDirective {
    const CAPABILITY: Capability = Capability::Production;
}

impl<R: OrderRefs> ProductionDirective<R> {
    /// The same directive with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<ProductionDirective<T>, M::Error> {
        Ok(match self {
            Self::CancelTrain { place } => ProductionDirective::CancelTrain { place },
            Self::Rally { target } => ProductionDirective::Rally {
                target: target.map(|target| target.map_refs(refs)).transpose()?,
            },
            Self::CancelBuild => ProductionDirective::CancelBuild,
            Self::Sell => ProductionDirective::Sell,
        })
    }
}
