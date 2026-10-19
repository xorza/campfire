use campfire_players::CapabilityAction;
use campfire_sim::Capability;
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

use crate::rally_target::RallyTarget;

/// What an order of `production` tells its units to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum ProductionAction<R: OrderRefs = Ids> {
    /// Cancel the entry at `place` of a producer's train queue, the head 0.
    CancelTrain { place: u8 },
    /// Set where a producer's trained units go, or clear it with `None`.
    Rally { target: Option<RallyTarget<R>> },
    /// Cancel a site of the player's building.
    CancelBuild,
}

impl Copy for ProductionAction {}

impl CapabilityAction for ProductionAction {
    const CAPABILITY: Capability = Capability::Production;
}

impl<R: OrderRefs> ProductionAction<R> {
    /// The same action with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<ProductionAction<T>, M::Error> {
        Ok(match self {
            Self::CancelTrain { place } => ProductionAction::CancelTrain { place },
            Self::Rally { target } => ProductionAction::Rally {
                target: target.map(|target| target.map_refs(refs)).transpose()?,
            },
            Self::CancelBuild => ProductionAction::CancelBuild,
        })
    }
}
