//! The client's data files of a package, which the sim never reads: how its camera moves, how its
//! unit types look, the materials that override its models' own, and the light its maps are drawn
//! in.

pub(crate) mod camera_file;
pub(crate) mod client_units;
pub(crate) mod map_lights;
pub(crate) mod material_file;
