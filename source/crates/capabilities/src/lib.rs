//! Capabilities: the mechanisms a mode combines, a module each, on the core, and the mode above
//! them. A module imports only from its own layer and the layers below, lowest first: `values`
//! (data value types); the core, `units`, `scripts` and `players` (unit types, teams, owners,
//! paths, the script view and runtime, and the players' resources); `stats`; `actions`; `combat`;
//! `deliveries`, `projectiles`, `areas`, `abilities`, `navigation`, `vision`, `progression` and
//! `production`; `orders`; the `mode`; and `capability_set`, which installs them. The layer test
//! in `capability_set` holds this list, and fails on any import from a higher layer. A capability
//! gives the script view its fields of a unit through its column, and the call frame what a call
//! reads besides the core's through its part.

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
mod actions;
mod areas;
mod books;
mod capability_set;
mod combat;
mod deliveries;
mod mode;
mod navigation;
mod orders;
mod players;
mod production;
mod progression;
mod projectiles;
mod scripts;
mod stats;
mod units;
mod values;
mod vision;

pub use abilities::Abilities;
pub use actions::Actions;
pub use actions::action_data::{ActionData, Range, RangeField, Targeting};
pub use actions::action_data_field::ActionDataField;
pub use actions::action_kind::ActionKind;
pub use actions::action_slots::{ActionSlot, ActionSlots, ActionTarget};
pub use actions::delivery_data::DeliveryData;
pub use actions::effect_data::{EffectData, EffectTo, Effecting, PlannedEffect};
pub use actions::error::{ActionError, ActionField};
pub use actions::slot_kind::SlotKind;

pub use areas::Areas;
pub use areas::area::Area;

pub use books::book_input::{BookInput, BookKind, BookPackage};
pub use books::error::BookError;
pub use books::package_content::PackageContent;
pub use books::unit_type_file::UnitTypeFile;
pub use books::{Books, ModeInputs};
pub use capability_set::CapabilitySet;

pub use combat::Combat;

pub use combat::combat_rules::CombatRules;
pub use combat::deaths::{DeathView, Deaths, Fallen};
pub use combat::on_death::OnDeath;

pub use combat::recent_attackers::RecentAttackers;
pub use combat::respawn::Respawn;
pub use mode::Mode;
pub use mode::choice_data::Offers;

pub use mode::error::{ModeError, UnitKitError};
pub use mode::map_data::MapData;
pub use mode::match_end::{MatchEnd, MatchResult};

pub use mode::mode_data::{ModeData, ModeParam};
pub use mode::mode_input::{InputValue, ModeInput};
pub use mode::mode_setup::ModeSetup;
pub use mode::mode_state::ModeState;
pub use mode::mode_state_decl::SyncTo;
pub use mode::offer::Offer;

pub use mode::team_manifest::TeamManifest;

pub use mode::unit_kit::UnitKit;
pub use navigation::Navigation;
pub use navigation::destination::Destination;
pub use navigation::error::MapProblem;

pub use navigation::path_walker::PathWalker;
pub use navigation::paths::Paths;
pub use navigation::progress::Progress;
pub use navigation::route::Route;
pub use navigation::walker::Walker;
pub use orders::Orders;

pub use orders::error::AiError;

pub use orders::order::{Action, Order};
pub use players::player_resources::PlayerResources;
pub use players::resource_id::ResourceId;
pub use production::Production;

pub use production::train_queue::TrainQueue;
pub use progression::Progression;
pub use progression::experience::Experience;

pub use projectiles::Projectiles;
pub use projectiles::projectile::Projectile;

pub use scripts::api_version::ApiVersion;
pub use scripts::error::CallError;
pub use scripts::hook::{Hook, ScriptRole};
pub use scripts::name_kind::NameKind;

pub use scripts::script_api::{ApiOwner, MemberKind, NameArgs, ScriptApi};
pub use scripts::script_book::ScriptBook;
pub use scripts::script_budgets::ScriptBudgets;
pub use scripts::script_failures::{ScriptFailure, ScriptFailures};
pub use scripts::script_limits::ScriptLimits;

pub use scripts::state_value::StateValue;
pub use stats::Stats;
pub use stats::error::ModifierProblem;
pub use stats::level::Level;
pub use stats::modifier_clocks::ModifierClocks;
pub use stats::modifier_data::ModifierData;
pub use stats::modifiers::Modifiers;
pub use stats::move_step::MoveStep;

pub use stats::pool_id::PoolId;
pub use stats::pools::Pools;

pub use stats::stat_graph::StatGraph;

pub use stats::stats_data::StatsData;
pub use units::Units;
pub use units::action_id::ActionId;

pub use units::body::Body;
pub use units::collision_data::CollisionData;
pub use units::dead::Dead;
pub use units::engine_tag::EngineTag;
pub use units::layer::Layer;
pub use units::modifier_id::ModifierId;
pub use units::owner::Owner;

pub use units::relations::Relations;
pub use units::spawn_point::SpawnPoint;

pub use units::team::Team;
pub use units::team_set::TeamSet;
pub use units::track_id::TrackId;

pub use units::unit_type::UnitType;
pub use units::unit_type_data::UnitTypeData;
pub use values::attitude::Attitude;
pub use values::bounds::Bounds;
pub use values::declared_name::DeclaredName;
pub use values::filter_data::FilterData;
pub use values::grid::Grid;
pub use values::metric::Metric;
pub use values::number::Number;
pub use values::param::{Param, Scaling};
pub use values::ranked::Ranked;

pub use values::scalar::Scalar;
pub use values::speed::Speed;
pub use values::stat::{EngineStat, Stat};
pub use vision::Vision;
pub use vision::seen_by::SeenBy;

#[cfg(any(test, feature = "internals"))]
pub mod internals {
    pub use crate::combat::internals::Arms;
    pub use crate::stats::internals::{carried, give_modifier};
    pub use crate::stats::pools::internals::spent;
    pub use crate::units::relations::internals::set_relation;
}

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::navigation::bench::collision;
}
