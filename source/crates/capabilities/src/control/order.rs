use campfire_math::Num;
use campfire_sim::{Capability, Command, StableId};

use crate::abilities::ability_slots::CastTarget;
use serde::{Deserialize, Serialize};

/// An order to one unit: the body of an `orders` command. Players, bots and AI issue the same
/// orders; an order to a group is one command per unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Order {
    pub unit: StableId,
    pub action: Action,
}

/// What an order tells its unit to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    /// Walk to a point on the ground plane.
    Move { x: Num, z: Num },
    /// Attack a unit until it dies or another order comes.
    Attack { target: StableId },
    /// Cast the ability in `slot` at `target`.
    Cast { slot: u8, target: CastTarget },
}

impl Order {
    /// The capability that orders go to.
    pub const CAPABILITY: Capability = Capability::Orders;

    pub fn encode(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("an order always encodes")
    }

    /// `None` unless the body is exactly one order: a client can send any bytes.
    pub fn decode(body: &[u8]) -> Option<Order> {
        let (order, rest) = postcard::take_from_bytes(body).ok()?;
        rest.is_empty().then_some(order)
    }

    /// A payload of `orders` alone, one command each.
    pub fn payload(orders: &[Order]) -> Vec<u8> {
        let bodies: Vec<Vec<u8>> = orders.iter().map(Order::encode).collect();
        Command::payload(Order::CAPABILITY, &bodies)
    }
}
