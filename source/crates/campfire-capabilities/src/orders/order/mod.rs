use campfire_common::{Binary, Taken};
use campfire_math::Num;
use campfire_sim::{Capability, Command, StableId};
use serde::{Deserialize, Serialize};

use crate::actions::action_target::ActionTarget;
use crate::items::item_id::ItemId;
use crate::orders::order::order_units::OrderUnits;
use crate::production::build_target::BuildTarget;
use crate::production::rally_target::RallyTarget;

pub(crate) mod order_units;

/// An order to the units it names: the body of an `orders` command. Players, bots and AI issue
/// the same orders, and an order to a group is one command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Order {
    pub units: OrderUnits,
    pub action: Action,
}

/// What an order tells its units to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Walk to a point on the ground plane; a group keeps its shape on a move away from it.
    Move { x: Num, z: Num },
    /// Attack a unit until it dies or another order comes.
    Attack { target: StableId },
    /// Start the action in `slot` at `target`: a cast, or a train.
    Slot { slot: u8, target: ActionTarget },
    /// Learn the next rank of the action in `slot`, for a point.
    Learn { slot: u8 },
    /// Buy one `item` at the mode's shop.
    Buy { item: ItemId },
    /// Sell the stack in inventory slot `slot` at the mode's shop.
    Sell { slot: u8 },
    /// Swap inventory slots `from` and `to`.
    Swap { from: u8, to: u8 },
    /// End what is under way, drop the target and the destination, and stand.
    Stop,
    /// Cancel the entry at `place` of a producer's train queue, the head 0.
    CancelTrain { place: u8 },
    /// Set where a producer's trained units go, or clear it with `None`.
    Rally { target: Option<RallyTarget> },
    /// Build with the build in `slot` at `target`.
    Build { slot: u8, target: BuildTarget },
    /// Cancel a site of the player's building.
    CancelBuild,
}

impl Action {
    /// Whether it becomes an order of each of its units, as the core checks and applies it: a
    /// move, an attack, a slot's action, a build and a stop. Learning, and the actions of
    /// production and items, apply in systems of their own.
    pub(crate) const fn to_units(self) -> bool {
        matches!(
            self,
            Action::Move { .. }
                | Action::Attack { .. }
                | Action::Slot { .. }
                | Action::Build { .. }
                | Action::Stop
        )
    }
}

impl Order {
    /// The capability that orders go to.
    pub const CAPABILITY: Capability = Capability::Orders;

    /// The order of `action` to `unit` alone.
    pub const fn one(unit: StableId, action: Action) -> Order {
        Order {
            units: OrderUnits::one(unit),
            action,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        Binary::encode(self)
    }

    /// `None` unless the body is exactly one order, to units in increasing stable id, each once:
    /// a client can send any bytes.
    pub fn decode(body: &[u8]) -> Option<Order> {
        let mut units = Vec::new();
        let action = Order::decode_into(body, &mut units)?;
        let units = OrderUnits::new(&units).expect("a decoded order's units rise");
        Some(Order { units, action })
    }

    /// Appends the units of the order `body` holds to `units`, and gives its action; `None`, with
    /// `units` as it was, unless the body is exactly one order, to at least one unit, in
    /// increasing stable id, each once.
    pub(crate) fn decode_into(body: &[u8], units: &mut Vec<StableId>) -> Option<Action> {
        let start = units.len();
        let action = Order::read(body, units);
        if action.is_none() {
            units.truncate(start);
        }
        action
    }

    fn read(body: &[u8], units: &mut Vec<StableId>) -> Option<Action> {
        let Taken {
            value: count,
            mut rest,
        } = Binary::take::<usize>(body).ok()?;
        // Each id takes a byte at least, so a count past the bytes left is a flaw, found before
        // it reserves anything.
        if count == 0 || count > rest.len() {
            return None;
        }
        let start = units.len();
        units.reserve(count);
        for _ in 0..count {
            let unit = Binary::take::<StableId>(rest).ok()?;
            units.push(unit.value);
            rest = unit.rest;
        }
        if !OrderUnits::rises(&units[start..]) {
            return None;
        }
        let Taken {
            value: action,
            rest,
        } = Binary::take::<Action>(rest).ok()?;
        rest.is_empty().then_some(action)
    }

    /// A payload of `orders` alone, one command each.
    pub fn payload(orders: &[Order]) -> Vec<u8> {
        let bodies: Vec<Vec<u8>> = orders.iter().map(Order::encode).collect();
        Command::payload(Order::CAPABILITY, &bodies)
    }

    /// Appends the payload of this order alone to `out`, its body encoded in `body`, a buffer the
    /// caller reuses, so a client that sends orders every tick allocates for none of them.
    pub fn write_payload(&self, body: &mut Vec<u8>, out: &mut Vec<u8>) {
        body.clear();
        Binary::encode_into(self, body);
        let command = Command {
            capability: Order::CAPABILITY,
            body,
        };
        Command::write(&[command], out);
    }
}

#[cfg(test)]
mod tests;
