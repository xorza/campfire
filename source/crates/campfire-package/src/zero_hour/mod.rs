//! What only a package imported from Zero Hour holds: the file formats its importer writes and its
//! client reads, which no other game shares, and the terrain's atlas, which the client draws by and
//! the install check checks.

pub(crate) mod class_texture;
pub(crate) mod error;
pub(crate) mod game_data;
pub(crate) mod terrain;
pub(crate) mod terrain_atlas;
