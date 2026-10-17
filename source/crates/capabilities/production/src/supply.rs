use std::any::TypeId;

use bevy_ecs::query::{Has, QueryData};
use campfire_script::RowReads;
use campfire_units::Dead;
use campfire_values::UnitType;

use crate::site::Site;
use crate::supply_cost_book::{SupplyUnit, UnitSupply};
use crate::train_queue::TrainQueue;

/// A player's supply: what it uses, and what its units give, before the mode's `max`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SupplySums {
    pub(crate) used: u64,
    pub(crate) given: u64,
}

/// The parts of a unit its supply counts, dead or not: its type, whether it is dead, its train
/// queue, and whether it is a site. A unit of no type counts for nothing.
#[derive(QueryData, Debug)]
#[query_data(derive(Debug))]
pub(crate) struct SupplyParts {
    unit_type: Option<&'static UnitType>,
    dead: Has<Dead>,
    queue: Option<&'static TrainQueue>,
    site: Has<Site>,
}

impl SupplyPartsItem<'_, '_> {
    /// The unit as its supply counts; none of no type.
    pub(crate) fn unit(&self) -> Option<SupplyUnit<'_>> {
        Some(SupplyUnit {
            unit_type: *self.unit_type?,
            dead: self.dead,
            complete: !self.site,
            queue: self.queue,
        })
    }

    /// The unit's type, when it is living and complete, as a requirement finds it.
    pub(crate) fn held(&self) -> Option<UnitType> {
        self.unit_type.copied().filter(|_| !self.dead && !self.site)
    }
}

impl RowReads for SupplyParts {
    type Changed = <(
        Option<&'static UnitType>,
        Has<Dead>,
        Option<&'static TrainQueue>,
        Has<Site>,
    ) as RowReads>::Changed;

    fn components(into: &mut Vec<(TypeId, &'static str)>) {
        <(
            Option<&'static UnitType>,
            Has<Dead>,
            Option<&'static TrainQueue>,
            Has<Site>,
        ) as RowReads>::components(into);
    }
}

impl SupplySums {
    /// Counts what a unit uses and gives, `counted`, to it.
    pub(crate) const fn add(&mut self, counted: UnitSupply) {
        self.used += counted.used;
        self.given += counted.given;
    }

    /// Takes out what a unit counted for, `counted`, from it.
    pub(crate) const fn take(&mut self, counted: UnitSupply) {
        self.used -= counted.used;
        self.given -= counted.given;
    }

    /// What its units give, at most `max`.
    pub(crate) fn cap(self, max: u64) -> u64 {
        self.given.min(max)
    }
}
