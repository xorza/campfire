//! Capabilities: the mechanisms a mode combines, a module each. A capability imports only from
//! the ones below it: `combat` from none; `navigation` and `abilities` from `combat`;
//! `control` from all three.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![allow(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod abilities;
mod combat;
mod control;
mod navigation;

pub use abilities::Abilities;
pub use abilities::ability_book::AbilityId;
pub use abilities::ability_data::{
    AbilityData, AbilityTables, Param, Range, Ranked, Relation, Scalar, Scaling, Targeting,
};
pub use abilities::ability_slots::{AbilitySlot, AbilitySlots, CastTarget};
pub use abilities::cast_failures::{CastFailure, CastFailures};
pub use abilities::error::{AbilityError, ApiError, CastError};
pub use abilities::resource_pool::ResourcePool;
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
