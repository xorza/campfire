use std::any::TypeId;

use bevy_ecs::query::{Has, QueryData};
use campfire_common::PlayerSlot;
use campfire_script::RowReads;
use campfire_units::{Dead, Owner};
use campfire_values::UnitType;

use crate::mode_supply_data::ModeSupplyData;
use crate::site::Site;
use crate::supply_cost_book::{SupplyCostBook, SupplyUnit, UnitSupply};
use crate::train_queue::TrainQueue;

/// Each player's supply, by slot, counted from the units and the queues: what its living units
/// and its queued trains use, and what its living, complete units give, at most the mode's `max`.
/// Derived, not state: a count reads the match as it stands, so it cannot drift from it.
#[derive(Debug, Default)]
pub(crate) struct Supply {
    players: Vec<SupplySums>,
    max: u64,
}

/// A player's supply: what it uses, and what its units give, before the mode's `max`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SupplySums {
    pub(crate) used: u64,
    given: u64,
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

/// The parts of a unit a player owns that supply counts, and that a requirement finds.
pub(crate) type Counted = (&'static Owner, SupplyParts);

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

impl Supply {
    /// Counts each player's supply from `units` anew, each as `costs` reads it, the cap at most
    /// `rules`' `max`.
    pub(crate) fn count<'w, 's>(
        &mut self,
        costs: &SupplyCostBook,
        rules: ModeSupplyData,
        units: impl IntoIterator<Item = (&'w Owner, SupplyPartsItem<'w, 's>)>,
    ) {
        self.players.clear();
        self.max = u64::from(rules.max);
        for (owner, parts) in units {
            if let Some(unit) = parts.unit() {
                let counted = costs.unit(unit);
                self.entry_mut(owner.slot()).add(counted);
            }
        }
    }

    /// What `slot` uses.
    pub(crate) fn used(&self, slot: PlayerSlot) -> u64 {
        self.players.get(slot.index()).map_or(0, |held| held.used)
    }

    /// What `slot`'s units give, at most the mode's `max`.
    pub(crate) fn cap(&self, slot: PlayerSlot) -> u64 {
        let held = self.players.get(slot.index()).copied();
        held.unwrap_or_default().cap(self.max)
    }

    /// Whether `slot` has room for `cost` more under its cap.
    pub(crate) fn has_room(&self, slot: PlayerSlot, cost: u32) -> bool {
        self.used(slot) + u64::from(cost) <= self.cap(slot)
    }

    /// Counts `cost` more used by `slot`, as a train joins its queue.
    pub(crate) fn reserve(&mut self, slot: PlayerSlot, cost: u32) {
        self.entry_mut(slot).used += u64::from(cost);
    }

    /// `slot`'s supply, to change, the players grown to hold it.
    fn entry_mut(&mut self, slot: PlayerSlot) -> &mut SupplySums {
        let at = slot.index();
        if self.players.len() <= at {
            self.players.resize(at + 1, SupplySums::default());
        }
        &mut self.players[at]
    }
}

impl SupplySums {
    /// Counts what a unit uses and gives, `counted`, to it.
    pub(crate) const fn add(&mut self, counted: UnitSupply) {
        self.used += counted.used;
        self.given += counted.given;
    }

    /// What its units give, at most `max`.
    pub(crate) fn cap(self, max: u64) -> u64 {
        self.given.min(max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cap_is_what_the_units_give_at_most_the_max_and_a_reserve_adds_to_what_is_used() {
        let mut supply = Supply {
            players: vec![
                SupplySums { used: 7, given: 30 },
                SupplySums { used: 2, given: 8 },
            ],
            max: 20,
        };
        let [first, second, absent] = [0, 1, 5].map(PlayerSlot::new);
        assert_eq!(
            [supply.cap(first), supply.cap(second), supply.cap(absent)],
            [20, 8, 0]
        );
        // 7 + 13 = 20 fits a cap of 20; 7 + 14 does not.
        assert!(supply.has_room(first, 13) && !supply.has_room(first, 14));
        supply.reserve(first, 13);
        assert_eq!(supply.used(first), 20);
        assert!(supply.has_room(first, 0) && !supply.has_room(first, 1));
        supply.reserve(absent, 3);
        assert_eq!((supply.used(absent), supply.cap(absent)), (3, 0));
    }
}
