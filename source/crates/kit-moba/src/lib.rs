//! MOBA kit: orders, abilities, modifiers, lanes, grid pathfinding and fog of war.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod controller;
mod destination;
mod moba_kit;
mod move_step;
mod order;

pub use controller::Controller;
pub use destination::Destination;
pub use moba_kit::MobaKit;
pub use move_step::MoveStep;
pub use order::Order;
