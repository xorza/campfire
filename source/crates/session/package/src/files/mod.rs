//! The schemas of package files that only the package load reads: manifests, and the data files
//! of avatars, loadout and unit types.

pub(crate) mod avatar_file;
pub(crate) mod bot_list;
pub(crate) mod content_table;
pub(crate) mod dependency;
pub(crate) mod dependency_path;
pub(crate) mod loadout_file;
pub(crate) mod manifest;
pub(crate) mod map_file;
pub(crate) mod mode_file;
pub(crate) mod mode_manifest;
pub(crate) mod package_header;
pub(crate) mod package_name;
pub(crate) mod rate_bounds;
pub(crate) mod rules_manifest;
pub(crate) mod script_list;
pub(crate) mod tick_range;
pub(crate) mod units_file;
