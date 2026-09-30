use campfire_math::Num;
use serde::{Deserialize, Serialize};

/// The payload of a MOBA input. Players, bots and AI issue the same orders.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Order {
    /// Walk to a point on the ground plane.
    Move { x: Num, z: Num },
}

impl Order {
    pub fn encode(&self) -> Vec<u8> {
        postcard::to_allocvec(self).expect("an order always encodes")
    }

    /// `None` unless the payload is exactly one order: a client can send any bytes.
    pub fn decode(payload: &[u8]) -> Option<Order> {
        let (order, rest) = postcard::take_from_bytes(payload).ok()?;
        rest.is_empty().then_some(order)
    }
}
