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

mod units;

pub use crate::units::Units;
pub use crate::units::arrived_short::ArrivedShort;
pub use crate::units::avatar::Avatar;
pub use crate::units::body::Body;
pub use crate::units::body_form::BodyForm;
pub use crate::units::body_unit::BodyUnit;
pub use crate::units::collision_data::CollisionData;
pub use crate::units::dead::Dead;
pub use crate::units::delivery_entity::DeliveryEntity;
pub use crate::units::destination::Destination;
pub use crate::units::entity_filters::UnitFilter;
pub use crate::units::entity_kinds::EntityKinds;
pub use crate::units::error::TypesError;
pub use crate::units::facing::{Facing, Turn};
pub use crate::units::forced_move::{Advanced, DashTo, ForcedMove, Goal};
pub use crate::units::hit_handle::HitHandle;
pub use crate::units::layer::Layer;
pub use crate::units::lifespan::Lifespan;
pub use crate::units::map_book::MapBook;
pub use crate::units::match_entity::MatchEntity;
pub use crate::units::move_step::MoveStep;
pub use crate::units::new_unit::NewUnit;
pub use crate::units::new_unit_states::NewUnitStates;
pub use crate::units::owner::Owner;
pub use crate::units::player::Player;
pub use crate::units::player_units::{Commanded, HeldPlayerUnits, PlayerUnits};
pub use crate::units::script_book::ScriptBook;
pub use crate::units::spawn_at::SpawnAt;
pub use crate::units::spawn_hooks::SpawnHooks;
pub use crate::units::spawn_point::SpawnPoint;
pub use crate::units::spawn_target::SpawnTarget;
pub use crate::units::spawner::Spawner;
pub use crate::units::status_tags::StatusTags;
pub use crate::units::tag_book::TagBook;
pub use crate::units::tag_data::TagData;
pub use crate::units::tag_effect::TagEffect;
pub use crate::units::turn_step::TurnStep;
pub use crate::units::type_origin_book::{TypeOrigin, TypeOriginBook};
pub use crate::units::unit_core::{MadeCore, UnitCore};
pub use crate::units::unit_entity::UnitEntity;
pub use crate::units::unit_grid::{GridUnits, UnitGrid, UnitsNear};
pub use crate::units::unit_state::{StateSync, SyncAll, SyncNone, SyncOwner, SyncTeam, UnitState};
pub use crate::units::unit_state_book::UnitStateBook;
pub use crate::units::unit_states::UnitStates;
pub use crate::units::unit_tracker::{TrackedUnit, TrackedUnits, Tracking, UnitTracker};
pub use crate::units::unit_type_book::UnitTypeBook;
pub use crate::units::unit_type_core::UnitTypeCore;
pub use crate::units::units_api::UnitsApi;
pub use crate::units::units_call::{StateWrite, UnitsCall};
pub use crate::units::units_world::{UnitsWorld, UnitsWorldCrate};
pub use crate::units::view_reader::ViewReader;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::units::internals::TEST_PACKAGE;
}
