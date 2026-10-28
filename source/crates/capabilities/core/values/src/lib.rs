//! Data value types: the numbers, per-rank values, params, filters and package paths that data
//! files write, the names and ids they resolve to, the small records a call carries, and the
//! components of a unit's identity that the script view reads: its team, its type, its tags,
//! and the teams' relations.

// A float's rounding and its NaNs differ by target and by a thread's flags, so the sim computes
// in none (07-determinism-core.md, One step: an `i128`, rounded once).
#![warn(clippy::float_arithmetic)]

mod action_id;
mod action_start;
mod allegiance;
mod bits256;
mod block;
mod damage_kind;
mod dash_delivery;
mod distance;
mod engine_enum;
mod engine_tag;
mod error;
mod filter_data;
mod hit;
mod item_id;
mod modifier_id;
mod name_kind;
mod name_table;
mod names;
mod number;
mod package_index;
mod package_name;
mod package_path;
mod param;
mod path_id;
mod perk_id;
mod player_view;
mod rank;
mod ranked;
mod relation;
mod relation_set;
mod relations;
mod resource_id;
mod scalar;
mod script_enum;
mod script_source;
mod share;
mod speed;
mod stat;
mod tag;
mod tag_filter_data;
mod tag_properties;
mod tag_property;
mod tag_set;
mod team;
mod team_set;
mod text_id;
mod track_id;
mod type_scope;
mod unit_tags;
mod unit_type;

pub use crate::action_id::ActionId;
pub use crate::action_start::ActionStart;
pub use crate::allegiance::Allegiance;
pub use crate::bits256::Bits256;
pub use crate::block::Block;
pub use crate::damage_kind::DamageKind;
pub use crate::dash_delivery::DashDelivery;
pub use crate::distance::Distance;
pub use crate::engine_enum::EngineEnum;
pub use crate::engine_tag::EngineTag;
pub use crate::error::{NamesError, ShareError, TimeTooLarge};
pub use crate::filter_data::{FilterData, FilterSyntax, FilterTag, TagTerm};
pub use crate::hit::Hit;
pub use crate::item_id::ItemId;
pub use crate::modifier_id::ModifierId;
pub use crate::name_kind::{Article, NameKind, NameScope, Named};
pub use crate::name_table::NameTable;
pub use crate::names::{MatchScope, NameId, NameKey, Names};
pub use crate::number::{Number, ParamRef};
pub use crate::package_index::PackageIndex;
pub use crate::package_name::PackageName;
pub use crate::package_path::PackagePath;
pub use crate::param::{Param, Scaling};
pub use crate::path_id::PathId;
pub use crate::perk_id::PerkId;
pub use crate::player_view::PlayerView;
pub use crate::rank::Rank;
pub use crate::ranked::Ranked;
pub use crate::relation::Relation;
pub use crate::relation_set::RelationSet;
pub use crate::relations::{Relations, VisionShare};
pub use crate::resource_id::ResourceId;
pub use crate::scalar::Scalar;
pub use crate::script_enum::ScriptEnum;
pub use crate::script_source::ScriptSource;
pub use crate::share::Share;
pub use crate::speed::Speed;
pub use crate::stat::{DeclaredStat, EngineStat, Stat};
pub use crate::tag::Tag;
pub use crate::tag_filter_data::TagFilterData;
pub use crate::tag_properties::TagProperties;
pub use crate::tag_property::TagProperty;
pub use crate::tag_set::TagSet;
pub use crate::team::Team;
pub use crate::team_set::TeamSet;
pub use crate::text_id::TextId;
pub use crate::track_id::TrackId;
pub use crate::type_scope::TypeScope;
pub use crate::unit_tags::{Motion, UnitTags};
pub use crate::unit_type::UnitType;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::script_enum::internals::named_as_data;
}
