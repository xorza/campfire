//! The readers of the original's formats: its archives, its packed streams, its maps, its
//! models, its INI files and its string table, below the conversions, which none of them reads.

pub(crate) mod big;
pub(crate) mod byte_cursor;
pub(crate) mod csf;
pub(crate) mod error;
pub(crate) mod game_float;
pub(crate) mod ini;
pub(crate) mod look_conditions;
pub(crate) mod map;
pub(crate) mod ref_pack;
pub(crate) mod w3d;
