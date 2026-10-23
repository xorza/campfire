use std::slice;

use bevy_ecs::resource::Resource;
use campfire_common::{PlayerSlot, Tick};
use campfire_declarations::{Report, Role};
use campfire_sim::StableId;

/// The orders of the tick `tick` that `check_player_orders` dropped, in input order, each for one
/// of its units, or for none when it had no unit to carry it, the drops of one order together:
/// what a host counts, as a client can send anything and a bot's order that no unit takes shows
/// only as a drop. Not state: it empties as each tick checks its orders.
#[derive(Resource, Debug, Default)]
pub struct DroppedOrders {
    tick: Tick,
    entries: Vec<DroppedOrder>,
    /// The order being checked.
    open: Option<OpenOrder>,
}

/// A drop of an order by one of its units, or by none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DroppedOrder {
    pub player_slot: PlayerSlot,
    /// The order's place among its player's orders of the tick.
    pub number: u32,
    /// None for an order that no unit can carry out at all.
    pub unit: Option<StableId>,
    pub cause: OrderDropCause,
    /// Whether no unit of its order took it.
    pub untaken: bool,
}

/// The order being checked: whose it is, its first drop, and the checked orders queued before it.
#[derive(Debug, Clone, Copy)]
struct OpenOrder {
    player_slot: PlayerSlot,
    number: u32,
    first: usize,
    queued: usize,
}

/// Why an order is dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderDropCause {
    /// None of its units is a living one its player controls that does not reset.
    NoUnit,
    /// A move or an attack-move none of whose units walks.
    NoWalker,
    /// An attack whose target is no living enemy that one of the unit's weapons selects.
    NoWeapon,
    /// Its unit target is one its player's vision group does not see.
    Hidden,
    /// A slot's action that is not a cast, a train, or a gather at a unit.
    SlotKind,
    /// A build whose slot holds no build.
    NoBuild,
}

impl DroppedOrders {
    /// The tick whose drops it holds.
    pub const fn tick(&self) -> Tick {
        self.tick
    }

    /// Each drop, in input order.
    pub fn iter(&self) -> slice::Iter<'_, DroppedOrder> {
        self.entries.iter()
    }

    /// The first drop of each order that no unit took, in input order.
    pub fn untaken(&self) -> impl Iterator<Item = &DroppedOrder> {
        let mut last = None;
        self.entries.iter().filter(move |drop| {
            let order = Some((drop.player_slot, drop.number));
            let first = last != order;
            last = order;
            first && drop.untaken
        })
    }

    /// Starts the record of `tick`.
    pub(crate) fn start(&mut self, tick: Tick) {
        debug_assert!(self.open.is_none(), "the last tick closed its last order");
        self.tick = tick;
        self.entries.clear();
    }

    /// Starts the drops of the order `number` of `player_slot`, with `queued` checked orders
    /// queued before it, and closes the order before it.
    pub(crate) fn open(&mut self, player_slot: PlayerSlot, number: u32, queued: usize) {
        self.close(queued);
        self.open = Some(OpenOrder {
            player_slot,
            number,
            first: self.entries.len(),
            queued,
        });
    }

    /// Records that `unit`, or no unit, drops the open order for `cause`.
    pub(crate) fn refuse(&mut self, unit: Option<StableId>, cause: OrderDropCause) {
        let open = self.open.expect("an order is open");
        self.entries.push(DroppedOrder {
            player_slot: open.player_slot,
            number: open.number,
            unit,
            cause,
            untaken: false,
        });
    }

    /// Closes the open order, if any, with `queued` checked orders queued by now: one that queued
    /// none is untaken.
    pub(crate) fn close(&mut self, queued: usize) {
        let Some(open) = self.open.take() else {
            return;
        };
        if queued == open.queued {
            debug_assert!(
                open.first < self.entries.len(),
                "an untaken order has a drop"
            );
            for drop in &mut self.entries[open.first..] {
                drop.untaken = true;
            }
        }
    }
}

impl<'a> IntoIterator for &'a DroppedOrders {
    type Item = &'a DroppedOrder;
    type IntoIter = slice::Iter<'a, DroppedOrder>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Report for DroppedOrders {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }
}
