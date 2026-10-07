use bevy_ecs::resource::Resource;

use crate::actions::action_book::ActionBook;
use crate::actions::kind_spec::KindSpec;
use crate::production::supply_data::SupplyData;
use crate::production::train_queue::TrainQueue;
use crate::units::action_id::ActionId;
use crate::units::by_type::ByType;
use crate::units::unit_type::UnitType;

/// What each unit type uses and gives of supply, and what the unit each train makes uses, by
/// the train's action: the one rule by which every count of supply reads a unit. Package data,
/// not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct SupplyCosts {
    types: ByType<SupplyData>,
    /// By action, 0 for an action that trains nothing.
    trains: Vec<u32>,
}

/// What one unit counts for: what it uses, its own cost while it lives and its queued trains',
/// and what it gives while it lives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct UnitSupply {
    pub(crate) used: u64,
    pub(crate) given: u64,
}

impl SupplyCosts {
    /// The costs of `types`, and of the trains of `actions`.
    pub(crate) fn new(types: ByType<SupplyData>, actions: &ActionBook) -> SupplyCosts {
        let trains = (0..)
            .map_while(|at| actions.get(ActionId::nth(at)))
            .map(|action| match action.kind {
                KindSpec::Train(made) => types.get(made).map_or(0, |supply| supply.cost),
                KindSpec::Cast | KindSpec::Attack(_) | KindSpec::Build(_) => 0,
            })
            .collect();
        SupplyCosts { types, trains }
    }

    /// What a unit of `unit_type` uses as it joins a queue.
    pub(crate) fn cost(&self, unit_type: UnitType) -> u32 {
        self.types.get(unit_type).map_or(0, |supply| supply.cost)
    }

    /// What a unit of `unit_type`, dead when `dead`, complete unless a site, with `queue`,
    /// counts for: a living unit uses its type's `cost`, and gives its `provides` once complete;
    /// a queued train, of a producer living or dead, uses its unit's `cost`.
    pub(crate) fn unit(
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
