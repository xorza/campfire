//! Capabilities: the mechanisms a mode combines, a module each, on the core, and the mode above
//! them. A module imports only from its own layer and the layers below, lowest first: `values`
//! (data value types); the core, `units`, `scripts` and `players` (unit types, teams, owners,
//! paths, the script view and runtime, and the players' resources); `stats`; `actions`; `combat`;
//! `deliveries`, `projectiles`, `areas`, `abilities`, `navigation`, `vision`, `progression`,
//! `production` and `items`; `orders`; the `mode`; and `capability_set`, which installs them, and
//! `books`, which builds a match's books. The layer test in `capability_set` holds this list, and
//! fails on any import from a higher layer. A capability gives the script view its fields of a
//! unit through its column, and the call frame what a call reads besides the core's through its
//! part.

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
mod geometry;
mod items;
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

pub use crate::abilities::Abilities;
pub use crate::actions::Actions;
pub use crate::actions::action_data::{ActionData, Toggle};
pub use crate::actions::action_data_field::ActionDataField;
pub use crate::actions::action_kind::ActionKind;
pub use crate::actions::action_range::ActionRange;
pub use crate::actions::action_slots::{ActionSlot, ActionSlots};
pub use crate::actions::action_target::ActionTarget;
pub use crate::actions::delivery_data::DeliveryData;
pub use crate::actions::effect_data::{
    DamageFields, EffectData, EffectTo, Effecting, HealFields, LaunchFields, ModifierFields,
    MoveData, PlannedEffect, PurgeFields, RestoreFields, SpawnFields, XpFields,
};
pub use crate::actions::error::ActionField;
pub use crate::actions::kind_data::KindData;
pub use crate::actions::range_field::RangeField;
pub use crate::actions::slot_kind::SlotKind;
pub use crate::actions::targeting::Targeting;
pub use crate::areas::Areas;
pub use crate::areas::area::Area;
pub use crate::books::book_input::{BookInput, BookKind, BookPackage};
pub use crate::books::error::BookError;
pub use crate::books::package_content::PackageContent;
pub use crate::books::type_place::TypePlace;
pub use crate::books::unit_type_file::UnitTypeFile;
pub use crate::books::{Books, ModeInputs};
pub use crate::capability_set::CapabilitySet;
pub use crate::combat::Combat;
pub use crate::combat::combat_rules::CombatRules;
pub use crate::combat::deaths::{DeathView, Deaths, Fallen};
pub use crate::combat::on_death::OnDeath;
pub use crate::combat::recent_attackers::RecentAttackers;
pub use crate::combat::respawn::Respawn;
pub use crate::geometry::bounds::Bounds;
pub use crate::geometry::grid::Grid;
pub use crate::geometry::metric::Metric;
pub use crate::geometry::polygon::Polygon;
pub use crate::geometry::polygon::error::PolygonError;
pub use crate::items::Items;
pub use crate::items::inventory::{Inventory, ItemStack};
pub use crate::items::inventory_data::InventoryData;
pub use crate::items::item_data::ItemData;
pub use crate::items::item_id::ItemId;
pub use crate::items::shop_data::ShopData;
pub use crate::mode::Mode;
pub use crate::mode::choice_data::Offers;
pub use crate::mode::error::{ModeError, UnitKitError};
pub use crate::mode::map_data::MapData;
pub use crate::mode::match_end::{MatchEnd, MatchResult};
pub use crate::mode::mode_data::{ModeData, ModeParam};
pub use crate::mode::mode_input::{InputValue, ModeInput};
pub use crate::mode::mode_setup::ModeSetup;
pub use crate::mode::mode_state::ModeState;
pub use crate::mode::offer::Offer;
pub use crate::mode::players_data::{Leaver, PlayersData};
pub use crate::mode::save_asked::SaveAsked;
pub use crate::mode::saves_data::{SaveBy, SavesData};
pub use crate::mode::team_manifest::TeamManifest;
pub use crate::mode::unit_kit::{InventorySpec, UnitKit};
pub use crate::navigation::Navigation;
pub use crate::navigation::destination::Destination;
pub use crate::navigation::error::MapProblem;
pub use crate::navigation::path_walker::PathWalker;
pub use crate::navigation::paths::Paths;
pub use crate::navigation::progress::Progress;
pub use crate::navigation::route::Route;
pub use crate::orders::Orders;
pub use crate::orders::error::AiError;
pub use crate::orders::learning::Learning;
pub use crate::orders::order::order_units::OrderUnits;
pub use crate::orders::order::{Action, Order};
pub use crate::players::player_resources::PlayerResources;
pub use crate::players::resource_id::ResourceId;
pub use crate::production::Production;
pub use crate::production::build_target::BuildTarget;
pub use crate::production::builder::{BuildOrder, Builder};
pub use crate::production::rally::Rally;
pub use crate::production::rally_target::RallyTarget;
pub use crate::production::site::Site;
pub use crate::production::train_queue::TrainQueue;
pub use crate::progression::Progression;
pub use crate::progression::experience::Experience;
pub use crate::progression::points::Points;
pub use crate::projectiles::Projectiles;
pub use crate::projectiles::projectile::Projectile;
pub use crate::projectiles::projectile_data::ProjectileHits;
pub use crate::scripts::api_version::ApiVersion;
pub use crate::scripts::applies::Applies;
pub use crate::scripts::error::{CallError, ParamProblem};
pub use crate::scripts::hook::Hook;
pub use crate::scripts::name_kind::NameKind;
pub use crate::scripts::script_api::api_owner::ApiOwner;
pub use crate::scripts::script_api::data_table::DataTable;
pub use crate::scripts::script_api::enum_record::EnumRecord;
pub use crate::scripts::script_api::member_spec::{EnumArgs, Forms, MemberSpec, NameArgs};
pub use crate::scripts::script_api::status::Status;
pub use crate::scripts::script_api::{MemberKind, ScriptApi};
pub use crate::scripts::script_book::ScriptBook;
pub use crate::scripts::script_budgets::ScriptBudgets;
pub use crate::scripts::script_failures::{ScriptFailure, ScriptFailures};
pub use crate::scripts::script_limits::ScriptLimits;
pub use crate::scripts::script_role::ScriptRole;
pub use crate::scripts::state_decl::synced_state_decl::SyncTo;
pub use crate::scripts::state_value::StateValue;
pub use crate::stats::Stats;
pub use crate::stats::error::ModifierProblem;
pub use crate::stats::level::Level;
pub use crate::stats::modifier_clocks::ModifierClocks;
pub use crate::stats::modifier_data::ModifierData;
pub use crate::stats::modifiers::Modifiers;
pub use crate::stats::pool_id::PoolId;
pub use crate::stats::pools::Pools;
pub use crate::stats::stat_graph::StatGraph;
pub use crate::stats::stat_id::StatId;
pub use crate::stats::stats_data::StatsData;
pub use crate::units::Units;
pub use crate::units::action_id::ActionId;
pub use crate::units::body::{Body, BodyForm};
pub use crate::units::collision_data::CollisionData;
pub use crate::units::dead::Dead;
pub use crate::units::engine_tag::EngineTag;
pub use crate::units::forced_move::{DashTo, ForcedMove};
pub use crate::units::layer::Layer;
pub use crate::units::lifespan::Lifespan;
pub use crate::units::modifier_id::ModifierId;
pub use crate::units::move_step::MoveStep;
pub use crate::units::owner::Owner;
pub use crate::units::player_units::{Commanded, HeldPlayerUnits, PlayerUnits};
pub use crate::units::relations::Relations;
pub use crate::units::spawn_point::SpawnPoint;
pub use crate::units::team::Team;
pub use crate::units::team_set::TeamSet;
pub use crate::units::track_id::TrackId;
pub use crate::units::unit_type::UnitType;
pub use crate::units::unit_type_data::UnitTypeData;
pub use crate::values::declared_name::DeclaredName;
pub use crate::values::engine_enum::EngineEnum;
pub use crate::values::error::TimeTooLarge;
pub use crate::values::filter_data::FilterData;
pub use crate::values::number::Number;
pub use crate::values::package_path::PackagePath;
pub use crate::values::param::{Param, Scaling};
pub use crate::values::rank::Rank;
pub use crate::values::ranked::Ranked;
pub use crate::values::relation::Relation;
pub use crate::values::scalar::Scalar;
pub use crate::values::share::Share;
pub use crate::values::speed::Speed;
pub use crate::values::stat::{DeclaredStat, EngineStat, Stat};
pub use crate::vision::Vision;
pub use crate::vision::seen_by::SeenBy;

#[cfg(any(test, feature = "internals"))]
pub mod internals {
    pub use crate::combat::internals::{Arms, queue_damage};
    pub use crate::geometry::kernel_scene::{Density, KernelScene};
    pub use crate::mode::internals::spawn_typed;
    pub use crate::scripts::script_batch::internals::read_view;
    pub use crate::stats::internals::{carried, give_modifier};
    pub use crate::stats::pools::internals::spent;
    pub use crate::units::body::internals::{reaches, reaches_bound, sinks_into};
    pub use crate::units::relations::internals::set_relation;
    pub use crate::vision::seen_by::internals::seen_by_all;
}

#[cfg(feature = "bench")]
pub mod bench {
    use criterion::Criterion;

    use crate::geometry::body_box;
    use crate::{navigation, production, vision};

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        body_box::bench::body_box(c);
        navigation::bench::collision(c);
        navigation::bench::pathing_grid(c);
        navigation::bench::route_planner(c);
        production::bench::gather(c);
        vision::bench::fog(c);
    }
}
