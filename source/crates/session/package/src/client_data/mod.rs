//! The client's data files of a package, which the sim never reads: how its camera moves, how its
//! unit types and its maps' client objects look, the materials that override its models' own,
//! the light its maps are drawn in, and the objects of each map the client alone draws.

pub(crate) mod camera_file;
pub(crate) mod client_objects;
pub(crate) mod client_units;
pub(crate) mod map_lights;
pub(crate) mod material_file;
