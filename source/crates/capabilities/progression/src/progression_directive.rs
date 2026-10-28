use campfire_players::CapabilityDirective;
use campfire_sim::Capability;
use campfire_units::{Ids, OrderRefs, RefMap, SerdeRefs};
use serde::{Deserialize, Serialize};

/// What an order of `progression` tells its units, or its player, to do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", bound = "R: SerdeRefs")]
pub enum ProgressionDirective<R: OrderRefs = Ids> {
    /// Learn the next rank of the action in `slot`, for a point.
    Learn { slot: R::Slot },
    /// Learn the perk `perk` for its player, for its cost, whatever units the order names
    /// (04-capabilities/progression.md, A science is a perk of a player).
    LearnPerk { perk: R::Perk },
}

impl Copy for ProgressionDirective {}

impl CapabilityDirective for ProgressionDirective {
    const CAPABILITY: Capability = Capability::Progression;
}

impl<R: OrderRefs> ProgressionDirective<R> {
    /// Whether its player carries it out, whatever units its order names: a perk's learn.
    pub const fn is_players(&self) -> bool {
        matches!(self, Self::LearnPerk { .. })
    }

    /// The same directive with each reference as `refs` gives it.
    pub fn map_refs<T: OrderRefs, M: RefMap<R, T>>(
        self,
        refs: &mut M,
    ) -> Result<ProgressionDirective<T>, M::Error> {
        Ok(match self {
            Self::Learn { slot } => ProgressionDirective::Learn {
                slot: refs.slot(slot)?,
            },
            Self::LearnPerk { perk } => ProgressionDirective::LearnPerk {
                perk: refs.perk(perk)?,
            },
        })
    }
}
