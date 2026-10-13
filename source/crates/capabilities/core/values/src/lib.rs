//! Data value types: the numbers, per-rank values, params, filters and package paths that data
//! files write, the names and ids they resolve to, and the small records a call carries.

#![deny(clippy::float_arithmetic, clippy::iter_over_hash_type)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod action_start;
mod damage_kind;
mod declared_name;
mod engine_enum;
mod error;
mod filter_data;
mod hit;
mod name_table;
mod names;
mod number;
mod package_path;
mod param;
mod rank;
mod ranked;
mod relation;
mod relation_set;
mod row_directory;
mod scalar;
mod script_enum;
mod share;
mod speed;
mod stat;

pub use crate::action_start::ActionStart;
pub use crate::damage_kind::DamageKind;
pub use crate::declared_name::DeclaredName;
pub use crate::engine_enum::EngineEnum;
pub use crate::error::{NamesError, TimeTooLarge};
pub use crate::filter_data::{FilterData, FilterSyntax, FilterTag, TagTerm};
pub use crate::hit::Hit;
pub use crate::name_table::NameTable;
pub use crate::names::{NameId, NameKey, Names};
pub use crate::number::{Number, ParamRef};
pub use crate::package_path::PackagePath;
pub use crate::param::{Param, Scaling};
pub use crate::rank::Rank;
pub use crate::ranked::Ranked;
pub use crate::relation::Relation;
pub use crate::relation_set::RelationSet;
pub use crate::row_directory::{LayerView, RowDirectory, RowEntries};
pub use crate::scalar::Scalar;
pub use crate::script_enum::ScriptEnum;
pub use crate::share::Share;
pub use crate::speed::Speed;
pub use crate::stat::{DeclaredStat, EngineStat, Stat};

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::script_enum::internals::named_as_data;
}
