use bevy_ecs::resource::Resource;
use campfire_actions::{ActionBook, KindSpec};
use campfire_core::{ActionId, Book, ByType, Frame, UnitType, View};
use campfire_sim::Capability;

use crate::production_column::ProductionColumn;
use crate::supply_data::SupplyData;
use crate::train_queue::TrainQueue;

/// What each unit type uses and gives of supply, and what the unit each train makes uses, by
/// the train's action: the one rule by which every count of supply reads a unit. Package data,
/// not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplyCostBook {
    types: ByType<SupplyData>,
    /// By action, 0 for an action that trains nothing.
    trains: Vec<u32>,
}

/// What one unit counts for: what it uses, its own cost while it lives and its queued trains',
/// and what it gives while it lives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnitSupply {
    pub used: u64,
    pub given: u64,
}

impl SupplyCostBook {
    /// The costs of `types`, and of the trains of `book`.
    pub fn new(types: ByType<SupplyData>, book: &ActionBook) -> SupplyCostBook {
        let trains = (0..)
            .map_while(|at| book.get(ActionId::new(at)))
            .map(|action| match action.kind {
                KindSpec::Train(made) => types.get(made).map_or(0, |supply| supply.cost),
                KindSpec::Cast | KindSpec::Attack(_) | KindSpec::Build(_) | KindSpec::Gather(_) => {
                    0
                }
            })
            .collect();
        SupplyCostBook { types, trains }
    }

    /// What a unit of `unit_type` uses as it joins a queue.
    pub(crate) fn cost(&self, unit_type: UnitType) -> u32 {
        self.types.get(unit_type).map_or(0, |supply| supply.cost)
    }

    /// What a unit of `unit_type`, dead when `dead`, complete unless a site, with `queue`,
    /// counts for: a living unit uses its type's `cost`, and gives its `provides` once complete;
    /// a queued train, of a producer living or dead, uses its unit's `cost`.
    pub fn unit(
        &self,
        unit_type: UnitType,
        dead: bool,
        complete: bool,
        queue: Option<&TrainQueue>,
    ) -> UnitSupply {
        let own = match self.types.get(unit_type) {
            Some(supply) if !dead => *supply,
            _ => SupplyData::default(),
        };
        let given = if complete { own.provides } else { 0 };
        let queued: u64 = queue
            .map_or(&[][..], TrainQueue::entries)
            .iter()
            .map(|queued| u64::from(self.trains[queued.action.index()]))
            .sum();
        UnitSupply {
            used: u64::from(own.cost) + queued,
            given: u64::from(given),
        }
    }
}

impl Book for SupplyCostBook {
    const OWNER: Option<Capability> = Some(Capability::Production);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, view: &View, _: Option<&mut Frame>) {
        ProductionColumn::share_costs(view, self.clone());
    }
}
