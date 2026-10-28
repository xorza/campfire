//! The units under every capability: their model, their books, their systems, their script
//! API, and the read of the script view.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]
#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]

mod arrived_short;
mod at_work;
mod avatar;
mod body;
mod body_form;
mod body_shape;
mod build_target;
mod collision_data;
mod damage_hits;
mod dead;
mod deaths;
mod declared_tags;
mod destination;
mod died;
mod due;
mod entity_filters;
mod entity_kinds;
mod error;
mod facing;
mod firing_rider;
mod flight_goal;
mod forced_move;
mod ground;
mod ground_data;
mod ground_point;
mod height_grid;
mod hit_handle;
mod layer;
mod leader;
mod leaving;
mod level_raised;
mod life;
mod lifespan;
mod living_spawns;
mod map_book;
mod map_point;
mod map_space;
mod meter;
mod move_step;
mod needed_level;
mod new_unit_states;
mod on_wings;
mod open_cell_run;
mod order_refs;
mod owner;
mod player_units;
mod pool_cost;
mod pool_id;
mod pool_purse;
mod row_marks;
mod row_source;
mod script_book;
mod seeing_rider;
mod seen_by;
mod source_reads;
#[cfg(feature = "internals")]
mod spawn;
mod spawn_at;
mod spawn_callbacks;
mod spawn_of;
mod spawn_point;
mod spawner;
mod stat_id;
mod state_write;
mod status_tags;
mod tag_book;
mod tag_data;
mod teleported;
mod track_levels;
mod turn_step;
mod type_origin_book;
mod unfinished;
mod unit_core;
mod unit_entity;
mod unit_grid;
mod unit_ordered;
mod unit_state;
mod unit_state_access;
mod unit_state_book;
mod unit_states;
mod unit_steps;
mod unit_ticks;
mod unit_tracker;
mod unit_type_book;
mod unit_type_core;
mod unit_types;
mod units;
mod units_api;
mod units_call;
mod units_column;
mod units_rules;
mod units_world;
mod variation;
mod variations;
mod view_reader;
mod walk;
mod wall;
mod wall_book;

pub use crate::arrived_short::ArrivedShort;
pub use crate::at_work::AtWork;
pub use crate::avatar::Avatar;
pub use crate::body::Body;
pub use crate::body_form::BodyForm;
pub use crate::body_shape::BodyShape;
pub use crate::build_target::BuildTarget;
pub use crate::collision_data::{CollisionData, VariationData, VariationsData};
pub use crate::damage_hits::{DamageHit, DamageHits};
pub use crate::dead::Dead;
pub use crate::deaths::{DeathView, DeathViews, Deaths, Fallen};
pub use crate::declared_tags::DeclaredTags;
pub use crate::destination::Destination;
pub use crate::died::Died;
pub use crate::due::Due;
pub use crate::entity_filters::{Acting, Sighted, UnitFilter};
pub use crate::entity_kinds::EntityKinds;
pub use crate::error::{GroundError, GroundLoadError, MapPointError, TypesError, UnitTypeError};
pub use crate::facing::{Facing, Turn};
pub use crate::firing_rider::FiringRider;
pub use crate::flight_goal::FlightGoal;
pub use crate::forced_move::{Advanced, DashTo, ForcedMove, Goal};
pub use crate::ground::{CellSplit, CoarseSpan, Ground};
pub use crate::ground_data::GroundData;
pub use crate::ground_point::GroundPoint;
pub use crate::height_grid::HeightGrid;
pub use crate::hit_handle::HitHandle;
pub use crate::layer::Layer;
pub use crate::leader::Leader;
pub use crate::leaving::Leaving;
pub use crate::level_raised::LevelRaised;
pub use crate::life::{Life, UnitLife, lives};
pub use crate::lifespan::Lifespan;
pub use crate::living_spawns::LivingSpawns;
pub use crate::map_book::{BoundsData, MapBook, MetricData};
pub use crate::map_point::MapPoint;
pub use crate::map_space::MapSpace;
pub use crate::meter::Meter;
pub use crate::move_step::MoveStep;
pub use crate::needed_level::NeededLevel;
pub use crate::on_wings::OnWings;
pub use crate::open_cell_run::{OpenCellAsk, OpenCellRun};
pub use crate::order_refs::{Ids, OrderRefs, RefMap, SerdeRefs};
pub use crate::owner::Owner;
pub use crate::player_units::{Commanded, HeldPlayerUnits, PlayerUnits};
pub use crate::pool_cost::PoolCost;
pub use crate::pool_id::PoolId;
pub use crate::pool_purse::{PoolPayer, PoolPurse};
pub use crate::script_book::ScriptBook;
pub use crate::seeing_rider::SeeingRider;
pub use crate::seen_by::SeenBy;
pub use crate::spawn_at::SpawnAt;
pub use crate::spawn_callbacks::SpawnCallbacks;
pub use crate::spawn_of::SpawnOf;
pub use crate::spawn_point::SpawnPoint;
pub use crate::spawner::Spawner;
pub use crate::stat_id::StatId;
pub use crate::status_tags::StatusTags;
pub use crate::tag_book::TagBook;
pub use crate::tag_book::tag_effect::TagEffect;
pub use crate::tag_data::{ModeTagsData, TagData};
pub use crate::teleported::Teleported;
pub use crate::track_levels::{TrackLevel, TrackLevels};
pub use crate::turn_step::TurnStep;
pub use crate::type_origin_book::{TypeOrigin, TypeOriginBook};
pub use crate::unfinished::Unfinished;
pub use crate::unit_core::UnitCore;
pub use crate::unit_entity::UnitEntity;
pub use crate::unit_entity::body_unit::BodyUnit;
pub use crate::unit_entity::delivery_entity::DeliveryEntity;
pub use crate::unit_grid::{GridBuild, GridUnits, UnitGrid, UnitsNear};
pub use crate::unit_ordered::{OrderKind, UnitOrdered};
pub use crate::unit_state::{
    AudienceAll, AudienceNone, AudienceOwner, AudienceTeam, StateAudience, UnitState,
};
pub use crate::unit_state_book::UnitStateBook;
pub use crate::unit_steps::UnitSteps;
pub use crate::unit_tracker::{TrackedUnit, TrackedUnits, Tracking, UnitTracker};
pub use crate::unit_type_book::UnitTypeBook;
pub use crate::unit_type_core::{TypeParamsData, TypeTagsData, UnitTypeCore};
pub use crate::unit_types::{UnitTypeBooks, UnitTypes};
pub use crate::units::{Units, UnitsSystems};
pub use crate::units_api::UnitsApi;
pub use crate::units_call::UnitsCall;
pub use crate::units_world::UnitsWorld;
pub use crate::variation::Variation;
pub use crate::variations::Variations;
pub use crate::view_reader::ViewReader;
pub use crate::walk::{Walk, WalkItem, WalkReadOnly, WalkReadOnlyItem};
pub use crate::wall::Wall;
pub use crate::wall_book::WallBook;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::spawn::Spawn;
    pub use crate::units::internals::TEST_PACKAGE;
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use crate::units_world::internals::UnitsWorldInternals;
}
