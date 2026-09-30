//! MOBA kit: orders, abilities, modifiers, lanes, grid pathfinding and fog of war.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![allow(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod attack_state;
mod attack_stats;
mod controller;
mod dead;
mod destination;
mod health;
mod lane_walker;
mod lanes;
mod moba_kit;
mod move_step;
mod order;
mod strikes;
mod team;
mod tower_ai;
mod unit_stats;
mod waves;

pub use attack_state::AttackState;
pub use attack_stats::AttackStats;
pub use controller::Controller;
pub use dead::Dead;
pub use destination::Destination;
pub use health::Health;
pub use lane_walker::LaneWalker;
pub use lanes::Lanes;
pub use moba_kit::MobaKit;
pub use move_step::MoveStep;
pub use order::Order;
pub use team::Team;
pub use tower_ai::TowerAi;
pub use unit_stats::UnitStats;
pub use waves::Waves;
