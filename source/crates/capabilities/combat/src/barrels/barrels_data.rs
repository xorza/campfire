use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_geometry::MAX_BOUND;
use campfire_math::{Num, Vec3};
use campfire_script::{ApiVersion, Status};
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::unit_offset::UnitOffset;

/// A unit type's `barrels` (04-capabilities/combat.md, A shot leaves its weapon's barrel): by the
/// name of each weapon of its package that has them, the barrels its shots leave from in turn.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BarrelsData(pub BTreeMap<DeclaredName, Vec<BarrelData>>);

/// A barrel: the point of its unit's frame, `at`, where a shot's projectile starts, and the way of
/// that frame, `way`, it leaves along, of any length but none, as data writes it, `[x, y, z]`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarrelData {
    pub(crate) at: UnitOffset,
    pub(crate) way: Vec3,
}

impl BarrelData {
    /// A barrel at `at` along `way`; `None` for a way of no length, or of a coordinate past
    /// `MAX_BOUND` either way.
    pub fn new(at: UnitOffset, [x, y, z]: [Num; 3]) -> Option<Self> {
        let bound = -MAX_BOUND..=MAX_BOUND;
        let way = Vec3::new(x, y, z);
        let held = [x, y, z]
            .iter()
            .all(|coordinate| bound.contains(coordinate))
            && way != Vec3::ZERO;
        held.then_some(Self { at, way })
    }
}

impl Serialize for BarrelData {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Fields {
            at: UnitOffset,
            way: [Num; 3],
        }
        let way = self.way;
        Fields {
            at: self.at,
            way: [way.x, way.y, way.z],
        }
        .serialize(serializer)
    }
}

/// Data is untrusted, so a way of no length, or past `MAX_BOUND`, fails to read.
impl<'de> Deserialize<'de> for BarrelData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            at: UnitOffset,
            way: [Num; 3],
        }
        let fields = Fields::deserialize(deserializer)?;
        Self::new(fields.at, fields.way).ok_or_else(|| {
            D::Error::custom("a barrel's way has a length, each coordinate within 2,048")
        })
    }
}

impl Section for BarrelsData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "barrels";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
