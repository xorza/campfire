//! Capabilities: the mechanisms a mode combines, a module each, on the core, and the mode above
//! them. A module imports only from the layers below its own, lowest first: `values` (data value
//! types); the core, `units` and `scripts` (unit types, teams, owners, paths, the script view and
//! runtime); `combat` and `stats`; `abilities`, `projectiles`, `navigation` and `vision`;
//! `orders`; the `mode`. A capability gives the script view its fields of a unit through a row
//! source.

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
mod capability_set;
mod combat;
mod mode;
mod navigation;
mod orders;
mod projectiles;
mod scripts;
mod stats;
mod units;
mod values;
mod vision;

pub use abilities::Abilities;
pub use abilities::ability_book::AbilityId;
pub use abilities::ability_data::{
    AbilityData, AreaData, AreaInside, ChannelData, ChargeData, ChargesData, ProjectileData, Range,
    RangeField, RankFields, Targeting, Toggle,
};
pub use abilities::ability_slots::{AbilitySlot, AbilitySlots, CastTarget};
pub use abilities::error::{AbilityError, AbilityField};
pub use capability_set::CapabilitySet;
pub use capability_set::error::CapabilityError;
pub use combat::Combat;
pub use combat::attack_state::AttackState;
pub use combat::attack_stats::AttackStats;
pub use combat::combat_data::{AttackData, CombatData};
pub use combat::combat_rules::{CombatRules, Leech};
pub use combat::combatant::Combatant;
pub use combat::dead::Dead;
pub use combat::deaths::{DeathView, Deaths, Fallen};
pub use combat::on_death::OnDeath;
pub use combat::recent_attackers::RecentAttackers;
pub use combat::respawn::Respawn;
pub use mode::Mode;
pub use mode::avatar_index::AvatarIndex;
pub use mode::error::{ModeError, UnitKitError};
pub use mode::loadout_index::LoadoutIndex;
pub use mode::map_data::{
    GridData, MapData, MapPoint, MarkerData, PathData, PlacedUnitData, RegionData,
};
pub use mode::match_end::{MatchEnd, MatchResult};
pub use mode::mode_data::{InputType, ListEntry, ModeData, ModeParam};
pub use mode::mode_input::{InputValue, ModeInput};
pub use mode::mode_setup::{AvatarSetup, LoadoutSetup, ModeSetup, UnitTypeSetup};
pub use mode::mode_state::ModeState;
pub use mode::picks::{Pick, Picks};
pub use mode::player_resources::{PlayerResource, PlayerResources};
pub use mode::relation_data::RelationData;
pub use mode::team_manifest::TeamManifest;
pub use mode::timers::{Timer, Timers};
pub use mode::unit_kit::{KitRules, UnitKit};
pub use navigation::Navigation;
pub use navigation::destination::Destination;
pub use navigation::error::MapProblem;
pub use navigation::move_step::MoveStep;
pub use navigation::navigation_rules::NavigationRules;
pub use navigation::on_path::OnPath;
pub use navigation::path_walker::{PathEnd, PathWalker};
pub use navigation::paths::Paths;
pub use navigation::progress::Progress;
pub use navigation::route::Route;
pub use navigation::walker::Walker;
pub use orders::Orders;
pub use orders::ai_data::AiData;
pub use orders::error::AiError;
pub use orders::next_think::NextThink;
pub use orders::order::{Action, Order};
pub use projectiles::Projectiles;
pub use projectiles::projectile::Projectile;
pub use scripts::error::{ApiError, CallError};
pub use scripts::hook::{Hook, ScriptRole};
pub use scripts::match_scripts::MatchScripts;
pub use scripts::role_set::RoleSet;
pub use scripts::script_api::{ApiMember, ApiOwner, MemberKind, MemberSpec, ScriptApi, Status};
pub use scripts::script_failures::{ScriptFailure, ScriptFailures};
pub use scripts::script_limits::ScriptLimits;
pub use scripts::state_decl::{StateDecl, StateDefault, StateType, SyncTo};
pub use scripts::state_value::StateValue;
pub use stats::Stats;
pub use stats::level::Level;
pub use stats::modifier_book::ModifierId;
pub use stats::modifier_data::{AuraData, ModifierData, Reapply};
pub use stats::modifiers::Modifiers;
pub use stats::pool_cost::PoolCost;
pub use stats::pool_data::PoolData;
pub use stats::pool_id::PoolId;
pub use stats::pools::Pools;
pub use stats::stat::{EngineStat, Stat};
pub use stats::stat_change::StatChange;
pub use stats::stat_graph::StatGraph;
pub use stats::stat_op::StatOp;
pub use stats::stat_rule::StatRule;
pub use stats::stats_data::{StatValue, StatsData};
pub use units::Units;
pub use units::block::Block;
pub use units::body::Body;
pub use units::collision_data::CollisionData;
pub use units::error::UnitTypeError;
pub use units::layer::Layer;
pub use units::owner::Owner;
pub use units::path_id::PathId;
pub use units::recent_attack::RecentAttack;
pub use units::spawn_point::SpawnPoint;
pub use units::tag_data::TagData;
pub use units::tag_effect::TagEffect;
pub use units::team::Team;
pub use units::team_set::TeamSet;
pub use units::unit_type::UnitType;
pub use units::unit_type_data::UnitTypeData;
pub use values::attitude::Attitude;
pub use values::bounds::Bounds;
pub use values::declared_name::DeclaredName;
pub use values::filter_data::FilterData;
pub use values::grid::Grid;
pub use values::metric::Metric;
pub use values::number::{Number, ParamRef};
pub use values::param::{Param, Scaling};
pub use values::ranked::Ranked;
pub use values::relation::Relation;
pub use values::scalar::Scalar;
pub use values::speed::Speed;
pub use vision::Vision;
pub use vision::seen_by::SeenBy;
pub use vision::sight::Sight;
pub use vision::vision_data::VisionData;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::stats::internals::{give_modifier, load_stats};
    pub use crate::stats::pools::internals::spent;
}

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::navigation::bench::collision;
}
