use campfire_players::CapabilityDirective;
use campfire_sim::Capability;
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

/// What an order of `progression` tells its units to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum ProgressionDirective<R: OrderRefs = Ids> {
    /// Learn the next rank of the action in `slot`, for a point.
    Learn { slot: R::Slot },
}

impl Copy for ProgressionDirective {}

impl CapabilityDirective for ProgressionDirective {
    const CAPABILITY: Capability = Capability::Progression;
}

impl<R: OrderRefs> ProgressionDirective<R> {
    /// The same directive with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<ProgressionDirective<T>, M::Error> {
        Ok(match self {
            Self::Learn { slot } => ProgressionDirective::Learn {
                slot: refs.slot(slot)?,
            },
        })
    }
}
