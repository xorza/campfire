use thiserror::Error;

use crate::zero_hour::format::error::CutShort;

/// Why a W3D file's bytes are no model the importer converts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum W3dError {
    /// A chunk or a field runs past its end.
    #[error("a chunk runs past its end")]
    Short,
    /// A name is not ASCII.
    #[error("a name is not ASCII")]
    NotAscii,
    /// A chunk lacks a part it needs, as a mesh its header.
    #[error("a chunk lacks a part")]
    Missing,
    /// An index passes its list, or a pivot's parent does not come before it.
    #[error("an index passes its list")]
    Index,
    /// A mesh's lists are not its header's counts.
    #[error("a mesh's lists are not its counts")]
    Counts,
    /// A float is infinite or not a number.
    #[error("a float is not finite")]
    NotFinite,
    /// An HLOD has other than one level of detail.
    #[error("an HLOD has {0} levels of detail")]
    Levels(u32),
    /// An HLOD of one level of detail has other than one array of it.
    #[error("an HLOD of one level of detail has {0} arrays of it")]
    LodArrays(u32),
    /// A mesh is of a geometry the importer does not draw, as a camera-aligned one.
    #[error("a mesh is of geometry {0:#x}")]
    Geometry(u32),
    /// A material pass holds more texture stages than the game's two.
    #[error("a pass holds more than two texture stages")]
    Stages,
}

impl From<CutShort> for W3dError {
    fn from(_: CutShort) -> Self {
        Self::Short
    }
}
