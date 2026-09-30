//! Capabilities: the mechanisms a mode combines, a module each, on the core `units`. A
//! capability imports only from the ones below it: `combat` from none; `units` from `combat`;
//! `navigation` from `combat`; `projectiles` and `abilities` from `combat` and `units`;
//! `control` from all of them.

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
mod projectiles;
mod units;

pub use abilities::Abilities;
pub use abilities::ability_book::AbilityId;
pub use abilities::ability_data::{
    AbilityData, AbilityTables, Param, Range, Ranked, Scaling, Targeting,
};
pub use abilities::ability_slots::{AbilitySlot, AbilitySlots, CastTarget};
pub use abilities::error::AbilityError;
pub use abilities::resource_pool::ResourcePool;
pub use combat::Combat;
pub use combat::attack_state::AttackState;
pub use combat::attack_stats::AttackStats;
pub use combat::combatant::Combatant;
pub use combat::dead::Dead;
pub use combat::health::Health;
pub use combat::on_death::OnDeath;
pub use combat::recent_attackers::{RecentAttack, RecentAttackers};
pub use combat::team::Team;
pub use control::Control;
pub use control::ai_data::AiData;
pub use control::controller::Controller;
pub use control::error::AiError;
pub use control::order::{Action, Order};
pub use navigation::Navigation;
pub use navigation::destination::Destination;
pub use navigation::lane_walker::{LaneWalker, PathDirection};
pub use navigation::lanes::Lanes;
pub use navigation::move_step::MoveStep;
pub use projectiles::Projectiles;
pub use projectiles::projectile::Projectile;
pub use units::Units;
pub use units::error::{ApiError, CallError, UnitTypeError};
pub use units::relation::Relation;
pub use units::scalar::Scalar;
pub use units::script_failures::{Hook, ScriptFailure, ScriptFailures};
pub use units::unit_type::UnitType;
pub use units::unit_type_data::UnitTypeData;
