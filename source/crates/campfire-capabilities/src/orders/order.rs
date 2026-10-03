use std::mem;

use campfire_math::Num;
use campfire_sim::{Capability, Command, StableId};
use serde::{Deserialize, Serialize};

use crate::actions::action_target::ActionTarget;

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
    /// Start the action in `slot` at `target`: a cast, or a train.
    Slot { slot: u8, target: ActionTarget },
    /// Learn the next rank of the action in `slot`, for a point.
    Learn { slot: u8 },
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

    /// Appends the payload of this order alone to `out`, its body encoded in `body`, a buffer the
    /// caller reuses, so a client that sends orders every tick allocates for none of them.
    pub fn write_payload(&self, body: &mut Vec<u8>, out: &mut Vec<u8>) {
        body.clear();
        *body = postcard::to_extend(self, mem::take(body)).expect("an order always encodes");
        let command = Command {
            capability: Order::CAPABILITY,
            body,
        };
        Command::write(&[command], out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id(value: u8) -> StableId {
        postcard::from_bytes(&[value]).unwrap()
    }

    #[test]
    fn orders_decode_exactly() {
        let order = Order {
            unit: id(4),
            action: Action::Move {
                x: Num::int(-3),
                z: Num::from_bits(5),
            },
        };
        let body = order.encode();
        assert_eq!(Order::decode(&body), Some(order));
        assert_eq!(Order::decode(&[body.as_slice(), &[0]].concat()), None);
        assert_eq!(Order::decode(&body[..body.len() - 1]), None);
        assert_eq!(Order::decode(&[]), None);

        // The unit's id, then variant 1 with the target's id, each a varint: 300 = 0xAC 0x02.
        let target = postcard::from_bytes::<StableId>(&[0xAC, 0x02]).unwrap();
        let attack = Order {
            unit: target,
            action: Action::Attack { target },
        };
        assert_eq!(attack.encode(), [0xAC, 0x02, 1, 0xAC, 0x02]);
        assert_eq!(Order::decode(&[0xAC, 0x02, 1, 0xAC, 0x02]), Some(attack));
        // Variant 2 does not exist.
        assert_eq!(Order::decode(&[0, 2, 0]), None);

        // A payload is a list of `orders` commands, one per order.
        let payload = Order::payload(&[order, attack]);
        let mut commands = Vec::new();
        assert!(Command::read(&payload, |command, _| commands.push(command)));
        assert_eq!(commands.len(), 2);
        assert!(
            commands
                .iter()
                .all(|command| command.capability == Capability::Orders)
        );
        assert_eq!(commands[1].body, attack.encode());

        // A written payload follows what the buffer holds: 1 command, of capability 5 (`orders`),
        // its 5 body bytes; then the move's payload, the body buffer reused.
        let mut out = vec![9];
        let mut body = Vec::new();
        attack.write_payload(&mut body, &mut out);
        assert_eq!(out, [9, 1, 5, 5, 0xAC, 0x02, 1, 0xAC, 0x02]);
        order.write_payload(&mut body, &mut out);
        assert_eq!(out[9..], Order::payload(&[order]));
    }
}
