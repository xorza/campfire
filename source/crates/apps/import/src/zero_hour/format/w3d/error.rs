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
    /// A mesh's lists are not its header's counts, or a channel's last frame comes before its
    /// first.
    #[error("a list is not its count")]
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
    /// An animation has no frame, or a rate of none a second.
    #[error("an animation has no frame or a rate of 0")]
    Timing,
    /// An animation's channel is of a type the importer does not convert, as an Euler turn, or
    /// of other than its type's floats a key.
    #[error("a channel is of type {kind} and {len} floats a key")]
    Channel { kind: u16, len: u16 },
    /// A compressed animation is of a flavor other than time-coded, as adaptive delta.
    #[error("a compressed animation is of flavor {0}")]
    Flavor(u16),
    /// A time-coded key steps to its value, which the importer does not convert.
    #[error("a time-coded key steps")]
    Step,
    /// A time-coded channel's keys do not rise from frame 0, or pass the last frame.
    #[error("a time-coded channel's keys do not rise from frame 0 within the frames")]
    TimeCodes,
    /// A compressed animation holds a time-coded visibility channel, which the importer does not
    /// convert.
    #[error("a compressed animation holds a time-coded visibility channel")]
    TimedVisibility,
}

impl From<CutShort> for W3dError {
    fn from(_: CutShort) -> Self {
        Self::Short
    }
}
