//! The core under every capability: the script runtime, how a module declares what it adds,
//! the units and the players. Its four modules still import each other, so they share one
//! crate.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod declare;
mod players;
mod scripts;
mod units;

pub use crate::declare::book::Book;
pub use crate::declare::cache::Cache;
pub use crate::declare::callbacks::Callbacks;
pub use crate::declare::capability_set::CapabilitySet;
pub use crate::declare::declarations::{ColumnFill, Declarations};
pub use crate::declare::entity_parts::EntityParts;
pub use crate::declare::error::CapabilityError;
pub use crate::declare::install_cx::InstallCx;
pub use crate::declare::kind_spawn::KindSpawn;
pub use crate::declare::kind_visitor::KindVisitor;
pub use crate::declare::module::{Module, ModuleVisitor};
pub use crate::declare::module_set::ModuleSet;
pub use crate::declare::per_type::PerType;
pub use crate::declare::phase::Phase;
pub use crate::declare::report::Report;
pub use crate::declare::scratch::Scratch;
pub use crate::declare::set_place::SetPlace;
pub use crate::declare::spawn_parts::SpawnParts;
pub use crate::players::Players;
pub use crate::players::capability_action::CapabilityAction;
pub use crate::players::held_out::HeldOut;
pub use crate::players::on_match::OnMatch;
pub use crate::players::order_units::OrderUnits;
pub use crate::players::out_of_play::{Avatars, OutOfPlay};
pub use crate::players::player_resources::PlayerResources;
pub use crate::players::player_rows::{PlayerRows, PlayerRowsMut};
pub use crate::players::players_world::{PlayersWorld, PlayersWorldCrate};
pub use crate::players::resource_amount::ResourceAmount;
pub use crate::players::tick_commands::{TickCommand, TickCommands};
pub use crate::scripts::api_builder::ApiBuilder;
pub use crate::scripts::api_version::ApiVersion;
pub use crate::scripts::applies::Applies;
pub use crate::scripts::arg_ctx::ArgCtx;
pub use crate::scripts::call_part::CallPart;
pub use crate::scripts::call_start::CallStart;
pub use crate::scripts::core_api::CoreApi;
pub use crate::scripts::ctx::{Ctx, Params};
pub use crate::scripts::ctx_fn::{CtxFn, Fallible, GetFn, MethodFn, Plain, WithCtx, WithoutCtx};
pub use crate::scripts::data_section::DataSection;
pub use crate::scripts::effects::{Effect, Effects};
pub use crate::scripts::error::{
    ApiError, CallError, Checked, CtxMisuse, ParamProblem, UnknownName,
};
pub use crate::scripts::frame::Frame;
pub use crate::scripts::hook::Hook;
pub use crate::scripts::hook_set::HookSet;
pub use crate::scripts::name_kind::NameKind;
pub use crate::scripts::pending_calls::PendingCalls;
pub use crate::scripts::pool::Pool;
pub use crate::scripts::role_set::RoleSet;
pub use crate::scripts::script_api::ScriptApi;
pub use crate::scripts::script_api::api_member::ApiMember;
pub use crate::scripts::script_api::api_owner::ApiOwner;
pub use crate::scripts::script_api::data_field::DataField;
pub use crate::scripts::script_api::data_table::DataTable;
pub use crate::scripts::script_api::enum_record::EnumRecord;
pub use crate::scripts::script_api::hook_status::HookStatus;
pub use crate::scripts::script_api::member_kind::MemberKind;
pub use crate::scripts::script_api::member_spec::{
    ArgKinds, EnumArgs, Forms, MemberSpec, NameArgs,
};
pub use crate::scripts::script_api::status::Status;
pub use crate::scripts::script_api::tag_property_status::TagPropertyStatus;
pub use crate::scripts::script_arg::{ScriptArg, TextArg};
pub use crate::scripts::script_args::ScriptArgs;
pub use crate::scripts::script_batch::ScriptBatch;
pub use crate::scripts::script_book::ScriptBook;
pub use crate::scripts::script_budgets::ScriptBudgets;
pub use crate::scripts::script_facts::{
    CtxUse, EnumPath, EnumString, MemberUse, ScriptFacts, ScriptFunction, ScriptName,
};
pub use crate::scripts::script_failures::{ScriptFailure, ScriptFailures};
pub use crate::scripts::script_fn::ScriptFn;
pub use crate::scripts::script_limits::ScriptLimits;
pub use crate::scripts::script_list::ScriptList;
pub use crate::scripts::script_map::ScriptMap;
pub use crate::scripts::script_names::ScriptNames;
pub use crate::scripts::script_role::ScriptRole;
pub use crate::scripts::script_text::ScriptText;
pub use crate::scripts::script_value::{ScriptType, ScriptValue};
pub use crate::scripts::scripts_world::ScriptsWorldCrate;
pub use crate::scripts::state_decl::synced_state_decl::{SyncTo, SyncedStateDecl};
pub use crate::scripts::state_decl::{StateDecl, StateDefault, StateType};
pub use crate::scripts::state_value::StateValue;
pub use crate::units::Units;
pub use crate::units::action_id::ActionId;
pub use crate::units::arrived_short::ArrivedShort;
pub use crate::units::avatar::Avatar;
pub use crate::units::bits256::Bits256;
pub use crate::units::block::Block;
pub use crate::units::body::Body;
pub use crate::units::body_form::BodyForm;
pub use crate::units::body_grid::{BodyGrid, Placed};
pub use crate::units::body_unit::BodyUnit;
pub use crate::units::by_type::ByType;
pub use crate::units::collision_data::CollisionData;
pub use crate::units::dead::Dead;
pub use crate::units::delivery_entity::DeliveryEntity;
pub use crate::units::destination::Destination;
pub use crate::units::engine_tag::EngineTag;
pub use crate::units::entity_filters::UnitFilter;
pub use crate::units::entity_kind::{EntityKind, KindAudience};
pub use crate::units::entity_kinds::EntityKinds;
pub use crate::units::error::TypesError;
pub use crate::units::facing::Facing;
pub use crate::units::filter::Filter;
pub use crate::units::forced_move::{Advanced, DashDelivery, DashTo, ForcedMove, Goal};
pub use crate::units::hit_handle::HitHandle;
pub use crate::units::kept_rows::{ColumnRows, KeptRows, RunMove};
pub use crate::units::layer::Layer;
pub use crate::units::lifespan::Lifespan;
pub use crate::units::living_unit::LivingUnit;
pub use crate::units::map_book::MapBook;
pub use crate::units::match_entity::MatchEntity;
pub use crate::units::modifier_id::ModifierId;
pub use crate::units::move_step::MoveStep;
pub use crate::units::new_unit::NewUnit;
pub use crate::units::new_unit_states::NewUnitStates;
pub use crate::units::owner::Owner;
pub use crate::units::path_id::PathId;
pub use crate::units::player::Player;
pub use crate::units::player_units::{Commanded, HeldPlayerUnits, PlayerUnits};
pub use crate::units::predicting::Predicting;
pub use crate::units::relations::Relations;
pub use crate::units::resource_id::ResourceId;
pub use crate::units::row_fill::RowFill;
pub use crate::units::row_reads::RowReads;
pub use crate::units::spawn_at::SpawnAt;
pub use crate::units::spawn_hooks::SpawnHooks;
pub use crate::units::spawn_point::SpawnPoint;
pub use crate::units::spawn_target::SpawnTarget;
pub use crate::units::spawner::Spawner;
pub use crate::units::status_tags::StatusTags;
pub use crate::units::tag::Tag;
pub use crate::units::tag_book::TagBook;
pub use crate::units::tag_data::TagData;
pub use crate::units::tag_effect::TagEffect;
pub use crate::units::tag_properties::TagProperties;
pub use crate::units::tag_property::TagProperty;
pub use crate::units::tag_set::TagSet;
pub use crate::units::team::Team;
pub use crate::units::team_set::TeamSet;
pub use crate::units::teams::Teams;
pub use crate::units::track_id::TrackId;
pub use crate::units::type_origin_book::{TypeOrigin, TypeOriginBook};
pub use crate::units::type_scope::TypeScope;
pub use crate::units::unit::Unit;
pub use crate::units::unit_core::{MadeCore, UnitCore};
pub use crate::units::unit_entity::UnitEntity;
pub use crate::units::unit_grid::{Built, UnitGrid, UnitsNear};
pub use crate::units::unit_row::UnitRow;
pub use crate::units::unit_rows::UnitRows;
pub use crate::units::unit_state::{StateSync, SyncAll, SyncNone, SyncOwner, SyncTeam, UnitState};
pub use crate::units::unit_state_book::{StateField, StatePlace, UnitStateBook};
pub use crate::units::unit_states::UnitStates;
pub use crate::units::unit_tags::UnitTags;
pub use crate::units::unit_type::UnitType;
pub use crate::units::unit_type_core::UnitTypeCore;
pub use crate::units::unit_types::UnitTypes;
pub use crate::units::units_api::UnitsApi;
pub use crate::units::units_call::{StateWrite, UnitsCall};
pub use crate::units::units_world::{UnitsWorld, UnitsWorldCrate};
pub use crate::units::view::View;
pub use crate::units::view_column::{ViewColumn, ViewColumns};
pub use crate::units::view_names::ViewNames;

#[cfg(feature = "internals")]
pub mod bench {
    use criterion::Criterion;

    use crate::units;

    /// Runs each bench of the crate whose id criterion's filter takes.
    pub fn run(c: &mut Criterion) {
        units::body_grid::bench::body_grid(c);
    }
}

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::scripts::error::internals::FailureKind;
    pub use crate::scripts::script_failures::internals::FailedCall;
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use crate::scripts::scripts_world::internals::ScriptsWorldInternals;
    pub use crate::units::internals::TEST_PACKAGE;
}
