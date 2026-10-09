//! Data value types: the numbers, per-rank values, params, filters and package paths that data
//! files write, the names and ids they resolve to, and the small records a call carries.

pub(crate) mod action_start;
pub(crate) mod binary_file;
pub(crate) mod damage_kind;
pub(crate) mod declared_name;
pub(crate) mod engine_enum;
pub(crate) mod error;
pub(crate) mod filter_data;
pub(crate) mod hit;
pub(crate) mod name_list;
pub(crate) mod name_table;
pub(crate) mod number;
pub(crate) mod package_path;
pub(crate) mod param;
pub(crate) mod rank;
pub(crate) mod ranked;
pub(crate) mod relation;
pub(crate) mod relation_set;
pub(crate) mod row_directory;
pub(crate) mod scalar;
pub(crate) mod script_enum;
pub(crate) mod share;
pub(crate) mod speed;
pub(crate) mod stat;
