//! Capabilities: the mechanisms a mode combines, a module each, on the core `units`, and the mode
//! above them. A capability imports only from the ones below it: `combat` and `stats` from none;
//! `units` from `combat`; `navigation` from `combat`; `projectiles` and `abilities` from `combat`
//! and `units`; `control` from all of them; the `mode` from every one.

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
mod mode;
mod navigation;
mod projectiles;
mod stats;
mod units;

pub use abilities::Abilities;
pub use abilities::ability_book::AbilityId;
pub use abilities::ability_data::{
    AbilityData, AreaData, AreaInside, ChannelData, ChargeData, ChargesData, ProjectileData, Range,
    Targeting, Toggle,
};
pub use abilities::ability_slots::{AbilitySlot, AbilitySlots, CastTarget};
pub use abilities::error::AbilityError;
pub use abilities::resource_pool::ResourcePool;
pub use combat::Combat;
pub use combat::attack_state::AttackState;
pub use combat::attack_stats::AttackStats;
pub use combat::combat_data::{AttackData, CombatData};
pub use combat::combatant::Combatant;
pub use combat::damage_kind::DamageKind;
pub use combat::dead::Dead;
pub use combat::health::Health;
pub use combat::on_death::OnDeath;
pub use combat::recent_attackers::{RecentAttack, RecentAttackers};
pub use combat::team::Team;
pub use control::Control;
pub use control::ai_data::AiData;
pub use control::controller::Controller;
pub use control::error::AiError;
pub use control::next_think::NextThink;
pub use control::order::{Action, Order};
pub use mode::Mode;
pub use mode::error::{ModeError, UnitKitError};
pub use mode::hero_data::{HeroData, ResourceKind};
pub use mode::manifest::{
    Backends, CollisionBackend, ContentManifest, Dependency, Manifest, ModeManifest,
    PathfindingBackend, TeamManifest, TickHzRange, VisibilityBackend,
};
pub use mode::map_data::{GroundPoint, LaneData, MapData, NeutralSpawnData, StructureData};
pub use mode::mode_data::{InputType, ListEntry, ModeData, ModeParam};
pub use mode::mode_input::{InputValue, ModeInput};
pub use mode::mode_setup::{HeroSetup, ModeSetup, SpellSetup, UnitTypeSetup};
pub use mode::mode_state::ModeState;
pub use mode::picks::{Pick, Picks};
pub use mode::player_resources::{PlayerResource, PlayerResources};
pub use mode::spells_data::SpellsData;
pub use mode::timers::{Timer, Timers};
pub use mode::unit_kit::{KitRules, UnitKit};
pub use mode::units_data::{UnitTypeFile, UnitsData, VisionData};
pub use navigation::Navigation;
pub use navigation::destination::Destination;
pub use navigation::lane_walker::{LaneWalker, PathDirection};
pub use navigation::lanes::Lanes;
pub use navigation::move_step::MoveStep;
pub use navigation::on_lane::OnLane;
pub use projectiles::Projectiles;
pub use projectiles::projectile::Projectile;
pub use stats::modifier_data::{AuraData, ModifierData, Reapply};
pub use stats::stat::Stat;
pub use stats::stats_data::{StatValue, StatsData};
pub use stats::unit_state::UnitState;
pub use units::Units;
pub use units::ctx_entry::{CtxEntry, CtxKind};
pub use units::error::{ApiError, CallError, UnitTypeError};
pub use units::filter::FilterSyntax;
pub use units::filter_data::FilterData;
pub use units::hook::{Hook, ScriptRole};
pub use units::number::{Number, ParamRef};
pub use units::param::{Param, Scaling};
pub use units::ranked::Ranked;
pub use units::relation::Relation;
pub use units::scalar::Scalar;
pub use units::script_failures::{ScriptFailure, ScriptFailures};
pub use units::script_limits::ScriptLimits;
pub use units::state_decl::{StateDecl, StateDefault, StateType, SyncTo};
pub use units::state_value::StateValue;
pub use units::unit_type::UnitType;
pub use units::unit_type_data::UnitTypeData;
