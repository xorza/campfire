use campfire_common::PlayerSlot;

use crate::production::supply_costs::SupplyCosts;
use crate::production::supply_rules::SupplyRules;
use crate::production::train_queue::TrainQueue;
use crate::units::unit_type::UnitType;

/// Each player's supply, by slot, counted from the units and the queues: what its living units
/// and its queued trains use, and what its living, complete units give, at most the mode's `max`.
/// Derived, not state: a count reads the match as it stands, so it cannot drift from it.
#[derive(Debug, Default)]
pub(crate) struct Supply {
    players: Vec<PlayerSupply>,
    max: u64,
}

/// A player's supply: what it uses, and what its units give, before the mode's `max`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PlayerSupply {
    used: u64,
    given: u64,
}

/// A unit as supply counts it: its type, its player, whether it is dead, and its train queue.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CountedUnit<'a> {
    pub(crate) unit_type: UnitType,
    pub(crate) owner: PlayerSlot,
    pub(crate) dead: bool,
    pub(crate) queue: Option<&'a TrainQueue>,
}

impl Supply {
    /// Counts each player's supply from `units` anew, each as `costs` reads it, the cap at most
    /// `rules`' `max`.
    pub(crate) fn count<'a>(
        &mut self,
        costs: &SupplyCosts,
        rules: SupplyRules,
        units: impl IntoIterator<Item = CountedUnit<'a>>,
    ) {
        self.players.clear();
        self.max = u64::from(rules.max);
        for unit in units {
            let at = unit.owner.index();
            if self.players.len() <= at {
                self.players.resize(at + 1, PlayerSupply::default());
            }
            let counted = costs.unit(unit.unit_type, unit.dead, unit.queue);
            self.players[at].used += counted.used;
            self.players[at].given += counted.given;
        }
    }

    /// What `player` uses.
    pub(crate) fn used(&self, player: PlayerSlot) -> u64 {
        self.players.get(player.index()).map_or(0, |held| held.used)
    }

    /// What `player`'s units give, at most the mode's `max`.
    pub(crate) fn cap(&self, player: PlayerSlot) -> u64 {
        let given = self
            .players
            .get(player.index())
            .map_or(0, |held| held.given);
        given.min(self.max)
    }

    /// Whether `player` has room for `cost` more under its cap.
    pub(crate) fn has_room(&self, player: PlayerSlot, cost: u32) -> bool {
        self.used(player) + u64::from(cost) <= self.cap(player)
    }

    /// Counts `cost` more used by `player`, as a train joins its queue.
    pub(crate) fn reserve(&mut self, player: PlayerSlot, cost: u32) {
        let at = player.index();
        if self.players.len() <= at {
            self.players.resize(at + 1, PlayerSupply::default());
        }
        self.players[at].used += u64::from(cost);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cap_is_what_the_units_give_at_most_the_max_and_a_reserve_adds_to_what_is_used() {
        let mut supply = Supply {
            players: vec![
                PlayerSupply { used: 7, given: 30 },
                PlayerSupply { used: 2, given: 8 },
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
