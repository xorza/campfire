use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_actions::{ActionBook, KindSpec};
use campfire_declare::{Book, ByType};
use campfire_script::View;
use campfire_values::{ActionId, NameId, UnitType};

use crate::supply_data::SupplyData;
use crate::train_queue::TrainQueue;

/// What each unit type uses and gives of supply, and what the unit each train makes uses, by
/// the train's action: the one rule by which every count of supply reads a unit. Package data,
/// not state. A clone shares the costs, as the script view reads them.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SupplyCostBook {
    types: Arc<ByType<SupplyData>>,
    /// By action, 0 for an action that trains nothing.
    trains: Arc<[u32]>,
}

/// What one unit counts for: what it uses, its own cost while it lives and its queued trains',
/// and what it gives while it lives.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnitSupply {
    pub used: u64,
    pub given: u64,
}

/// A unit as its supply counts: its type, whether it is dead, whether it is complete, not a
/// site, and its train queue.
#[derive(Debug, Clone, Copy)]
pub struct SupplyUnit<'a> {
    pub unit_type: UnitType,
    pub dead: bool,
    pub complete: bool,
    pub queue: Option<&'a TrainQueue>,
}

impl SupplyCostBook {
    /// The costs of `types`, and of the trains of `book`.
    pub fn new(types: ByType<SupplyData>, book: &ActionBook) -> Self {
        let trains = (0..)
            .map_while(|at| book.get(ActionId::new(at)))
            .map(|action| match action.kind {
                KindSpec::Train(made) => types.get(made).map_or(0, |supply| supply.cost),
                KindSpec::Cast
                | KindSpec::Attack(_)
                | KindSpec::Use
                | KindSpec::Build(_)
                | KindSpec::Gather(_) => 0,
            })
            .collect();
        Self {
            types: Arc::new(types),
            trains,
        }
    }

    /// Whether `other` is this book as one load made it, not a book of another load: the costs a
    /// row filled from it holds.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.types, &other.types) && Arc::ptr_eq(&self.trains, &other.trains)
    }

    /// What a unit of `unit_type` uses as it joins a queue.
    pub(crate) fn cost(&self, unit_type: UnitType) -> u32 {
        self.types.get(unit_type).map_or(0, |supply| supply.cost)
    }

    /// What `unit` counts for: a living unit uses its type's `cost`, and gives its `provides`
    /// once complete; a queued train, of a producer living or dead, uses its unit's `cost`.
    pub fn unit(&self, unit: SupplyUnit<'_>) -> UnitSupply {
        let SupplyUnit {
            unit_type,
            dead,
            complete,
            queue,
        } = unit;
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
    const SCRIPTS_ONLY: bool = false;

    /// The production column takes it from the match at each read.
    fn share(&self, _: &View) {}
}
