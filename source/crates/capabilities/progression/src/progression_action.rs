use campfire_players::CapabilityAction;
use campfire_sim::Capability;
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

/// What an order of `progression` tells its units to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum ProgressionAction<R: OrderRefs = Ids> {
    /// Learn the next rank of the action in `slot`, for a point.
    Learn { slot: R::Slot },
}

impl Copy for ProgressionAction {}

impl CapabilityAction for ProgressionAction {
    const CAPABILITY: Capability = Capability::Progression;
}

impl<R: OrderRefs> ProgressionAction<R> {
    /// The same action with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<ProgressionAction<T>, M::Error> {
        Ok(match self {
            Self::Learn { slot } => ProgressionAction::Learn {
                slot: refs.slot(slot)?,
            },
        })
    }
}
