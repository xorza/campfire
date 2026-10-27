use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_values::PackagePath;
use serde::{Deserialize, Serialize};

/// What a tracked or wheeled unit's treads do as it drives on the ground: the particle systems they
/// run, each by its name in the effects file, and the track they lay (02-engine-core.md, A tread
/// throws dust while it drives; 02-engine-core.md, A tread lays track marks).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TreadLook {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<DeclaredName>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackLook>,
}

/// A tread's track: the texture its marks draw, and their width in metres.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackLook {
    pub texture: PackagePath,
    pub width: Num,
}

impl TreadLook {
    pub const fn is_empty(&self) -> bool {
        self.systems.is_empty() && self.track.is_none()
    }
}
