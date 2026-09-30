//! Capabilities: the mechanisms a mode combines, a module each, on the core, and the mode above
//! them. A module imports only from the layers below its own, lowest first: `values` (data value
//! types); the core, `units` and `scripts` (unit types, teams, owners, lanes, the script view and
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
pub use abilities::resource_pool::ResourcePool;
pub use capability_set::CapabilitySet;
pub use capability_set::error::CapabilityError;
pub use combat::Combat;
pub use combat::attack_state::AttackState;
pub use combat::attack_stats::AttackStats;
pub use combat::combat_data::{AttackData, CombatData};
pub use combat::combatant::Combatant;
pub use combat::dead::Dead;
pub use combat::health::Health;
pub use combat::on_death::OnDeath;
pub use combat::recent_attackers::RecentAttackers;
pub use combat::respawn::Respawn;
pub use mode::Mode;
pub use mode::error::{ModeError, UnitKitError};
pub use mode::hero_index::HeroIndex;
pub use mode::map_data::{GroundPoint, LaneData, MapData, NeutralSpawnData, StructureData};
pub use mode::mode_data::{InputType, ListEntry, ModeData, ModeParam};
pub use mode::mode_input::{InputValue, ModeInput};
pub use mode::mode_setup::{HeroSetup, ModeSetup, SpellSetup, UnitTypeSetup};
pub use mode::mode_state::ModeState;
pub use mode::picks::{Pick, Picks};
pub use mode::player_resources::{PlayerResource, PlayerResources};
pub use mode::spell_index::SpellIndex;
pub use mode::team_manifest::TeamManifest;
pub use mode::timers::{Timer, Timers};
pub use mode::unit_kit::{KitRules, UnitKit};
pub use navigation::Navigation;
pub use navigation::destination::Destination;
pub use navigation::lane_walker::{LaneWalker, PathDirection};
pub use navigation::lanes::Lanes;
pub use navigation::move_step::MoveStep;
pub use navigation::on_lane::OnLane;
pub use orders::Orders;
pub use orders::ai_data::AiData;
pub use orders::error::AiError;
pub use orders::next_think::NextThink;
pub use orders::order::{Action, Order};
pub use projectiles::Projectiles;
pub use projectiles::projectile::Projectile;
pub use scripts::ctx_entry::{CtxEntry, CtxKind};
pub use scripts::error::{ApiError, CallError};
pub use scripts::hook::{Hook, ScriptRole};
pub use scripts::match_scripts::MatchScripts;
pub use scripts::script_failures::{ScriptFailure, ScriptFailures};
pub use scripts::script_limits::ScriptLimits;
pub use scripts::state_decl::{StateDecl, StateDefault, StateType, SyncTo};
pub use scripts::state_value::StateValue;
pub use stats::modifier_data::{AuraData, ModifierData, Reapply};
pub use stats::stat::{EngineStat, Stat};
pub use stats::stats_data::{StatValue, StatsData};
pub use stats::unit_state::UnitState;
pub use units::Units;
pub use units::error::UnitTypeError;
pub use units::lane::Lane;
pub use units::owner::Owner;
pub use units::recent_attack::RecentAttack;
pub use units::spawn_point::SpawnPoint;
pub use units::team::Team;
pub use units::team_set::TeamSet;
pub use units::unit_type::UnitType;
pub use units::unit_type_data::UnitTypeData;
pub use values::declared_name::DeclaredName;
pub use values::filter_data::FilterData;
pub use values::grid::Grid;
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
