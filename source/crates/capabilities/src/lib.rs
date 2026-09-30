//! Capabilities: the mechanisms a mode combines, a module each. A capability imports only from
//! the ones below it: `combat` from none, `navigation` from `combat`, `control` from both.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![allow(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod combat;
mod control;
mod navigation;

pub use combat::Combat;
pub use combat::attack_state::AttackState;
pub use combat::attack_stats::AttackStats;
pub use combat::combatant::Combatant;
pub use combat::dead::Dead;
pub use combat::health::Health;
pub use combat::on_death::OnDeath;
pub use combat::team::Team;
pub use control::Control;
pub use control::controller::Controller;
pub use control::order::{Action, Order};
pub use control::tower_ai::TowerAi;
pub use navigation::Navigation;
pub use navigation::destination::Destination;
pub use navigation::lane_walker::{LaneWalker, PathDirection};
pub use navigation::lanes::Lanes;
pub use navigation::move_step::MoveStep;
