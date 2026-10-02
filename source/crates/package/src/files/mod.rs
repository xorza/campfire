//! The schemas of package files that only the package load reads: manifests, and the data files
//! of avatars, loadout and unit types.

pub(crate) mod avatar_data;
pub(crate) mod backends;
pub(crate) mod dependency;
pub(crate) mod manifest;
pub(crate) mod mode_file;
pub(crate) mod mode_manifest;
pub(crate) mod package_content;
pub(crate) mod package_header;
pub(crate) mod tick_range;
pub(crate) mod units_data;
pub(crate) mod version;
