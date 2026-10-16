use campfire_capabilities::{Match, Order};
use campfire_common::Tick;
use campfire_harness::{FixedMatch, MatchUnits};
use campfire_runner::SessionWorld;
use campfire_sim::StableId;
use campfire_stats::Life;
use campfire_units::Units;
use campfire_values::UnitType;

use crate::failure::Failure;

/// America's command center, the building a skirmish starts a player with.
pub(crate) const CENTER: &str = "americacommandcenter";
/// America's dozer, the unit a skirmish starts a player with.
pub(crate) const DOZER: &str = "americavehicledozer";

/// A match a play of the check drives a tick at a time, as player 1, and the tick it runs next.
/// A part of the imported mode a play reads and does not find is a failure of the import.
#[derive(Debug)]
pub(crate) struct MatchPlay {
    fixed: FixedMatch,
    tick: Tick,
}

impl MatchPlay {
    /// `fixed`, before its first tick.
    pub(crate) const fn new(fixed: FixedMatch) -> Self {
        Self {
            fixed,
            tick: Tick::new(0),
        }
    }

    /// The tick it runs next: how many ticks it ran.
    pub(crate) const fn tick(&self) -> Tick {
        self.tick
    }

    pub(crate) const fn game(&self) -> &Match {
        self.fixed.game()
    }

    /// Runs the tick, and moves on to the next.
    pub(crate) fn step(&mut self) {
        self.fixed.game_mut().run_tick_alone();
        self.tick = Tick::new(self.tick.get() + 1);
    }

    /// Sends player 1's `orders`, stamped for the tick it runs next.
    pub(crate) fn order(&mut self, orders: &[Order]) {
        self.fixed.send(0, self.tick, &Order::payload(orders));
    }

    /// The living units player `slot` owns of the type `name`, by stable id; a failure when the
    /// mode has no such type.
    pub(crate) fn owned(&self, name: &'static str, slot: u32) -> Result<Vec<StableId>, Failure> {
        let world = self.fixed.game();
        let typed: UnitType = Units::type_named(world, name).ok_or(Failure::NoType(name))?;
        let owned = MatchUnits::of(world)
            .all()
            .filter(|(_, unit)| {
                unit.get::<UnitType>() == Some(&typed)
                    && MatchUnits::owned_by(unit, slot)
                    && unit.get::<Life>().is_none_or(|life| !life.is_empty())
            })
            .map(|(id, _)| id)
            .collect();
        Ok(owned)
    }

    /// Ends the match: it takes the log's events while it lives, as its harness checks them, so
    /// it ends before the check logs again.
    pub(crate) fn end(mut self) {
        self.fixed.end();
    }
}
