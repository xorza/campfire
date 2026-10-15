//! The units under every capability: their model, their books, their systems, their script
//! API, and the read of the script view.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod arrived_short;
mod avatar;
mod body;
mod body_form;
mod collision_data;
mod dead;
mod destination;
mod entity_filters;
mod entity_kinds;
mod error;
mod facing;
mod forced_move;
mod hit_handle;
mod layer;
mod lifespan;
mod map_book;
mod match_entity;
mod move_step;
mod new_unit;
mod new_unit_states;
mod owner;
mod player;
mod player_units;
mod row_marks;
mod row_source;
mod script_book;
mod source_reads;
mod spawn_at;
mod spawn_hooks;
mod spawn_point;
mod spawner;
mod state_write;
mod status_tags;
mod tag_book;
mod tag_data;
mod turn_step;
mod type_origin_book;
mod unit_core;
mod unit_entity;
mod unit_grid;
mod unit_state;
mod unit_state_access;
mod unit_state_book;
mod unit_states;
mod unit_ticks;
mod unit_tracker;
mod unit_type_book;
mod unit_type_core;
mod units;
mod units_api;
mod units_call;
mod units_column;
mod units_world;
mod view_reader;

pub use crate::arrived_short::ArrivedShort;
pub use crate::avatar::Avatar;
pub use crate::body::Body;
pub use crate::body_form::BodyForm;
pub use crate::collision_data::CollisionData;
pub use crate::dead::Dead;
pub use crate::destination::Destination;
pub use crate::entity_filters::UnitFilter;
pub use crate::entity_kinds::EntityKinds;
pub use crate::error::TypesError;
pub use crate::facing::{Facing, Turn};
pub use crate::forced_move::{Advanced, DashTo, ForcedMove, Goal};
pub use crate::hit_handle::HitHandle;
pub use crate::layer::Layer;
pub use crate::lifespan::Lifespan;
pub use crate::map_book::MapBook;
pub use crate::match_entity::MatchEntity;
pub use crate::move_step::MoveStep;
pub use crate::new_unit::NewUnit;
pub use crate::new_unit_states::NewUnitStates;
pub use crate::owner::Owner;
pub use crate::player::Player;
pub use crate::player_units::{Commanded, HeldPlayerUnits, PlayerUnits};
pub use crate::script_book::ScriptBook;
pub use crate::spawn_at::SpawnAt;
pub use crate::spawn_hooks::SpawnHooks;
pub use crate::spawn_point::SpawnPoint;
pub use crate::spawner::Spawner;
pub use crate::state_write::StateWrite;
pub use crate::status_tags::StatusTags;
pub use crate::tag_book::TagBook;
pub use crate::tag_book::tag_effect::TagEffect;
pub use crate::tag_data::TagData;
pub use crate::turn_step::TurnStep;
pub use crate::type_origin_book::{TypeOrigin, TypeOriginBook};
pub use crate::unit_core::UnitCore;
pub use crate::unit_entity::UnitEntity;
pub use crate::unit_entity::body_unit::BodyUnit;
pub use crate::unit_entity::delivery_entity::DeliveryEntity;
pub use crate::unit_grid::{GridUnits, UnitGrid, UnitsNear};
pub use crate::unit_state::{StateSync, SyncAll, SyncNone, SyncOwner, SyncTeam, UnitState};
pub use crate::unit_state_book::UnitStateBook;
pub use crate::unit_states::UnitStates;
pub use crate::unit_tracker::{TrackedUnit, TrackedUnits, Tracking, UnitTracker};
pub use crate::unit_type_book::UnitTypeBook;
pub use crate::unit_type_core::UnitTypeCore;
pub use crate::units::Units;
pub use crate::units_api::UnitsApi;
pub use crate::units_call::UnitsCall;
pub use crate::units_world::{UnitsWorld, UnitsWorldCrate};
pub use crate::view_reader::ViewReader;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::units::internals::TEST_PACKAGE;
}
