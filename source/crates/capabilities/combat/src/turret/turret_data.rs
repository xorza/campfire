use campfire_common::DeclaredName;
use campfire_declarations::{PatchError, Patched, Section, SectionTable};
use campfire_math::Num;
use campfire_script::{ApiVersion, Status};
use campfire_script::{DataSection, DataTable};
use serde::{Deserialize, Deserializer, Serialize};

/// A unit type's `turret` section (04-capabilities/combat.md, A unit type may have a turret):
/// its attacks of the slot kind `slot` turn with a turret, `turn_rate` degrees a second, and
/// start within `tolerance_deg` of their target's bearing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TurretData {
    pub(crate) turn_rate: Num,
    pub(crate) tolerance_deg: Num,
    pub(crate) slot: DeclaredName,
}

impl TurretData {
    /// A turret of `turn_rate` degrees a second and `tolerance_deg`, of the slot kind `slot`;
    /// `None` for a turn rate that is not positive, or a tolerance outside 0 to 180 degrees.
    pub fn new(turn_rate: Num, tolerance_deg: Num, slot: DeclaredName) -> Option<Self> {
        let held = turn_rate > Num::ZERO && (Num::ZERO..=Num::int(180)).contains(&tolerance_deg);
        held.then_some(Self {
            turn_rate,
            tolerance_deg,
            slot,
        })
    }
}

/// A unit type's `turret` as a tool writes it in part, over the defaults it lies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurretPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_rate: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tolerance_deg: Option<Num>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<DeclaredName>,
}

/// Data is untrusted, so a turn rate that is not positive, or a tolerance outside 0 to 180
/// degrees, fails to read.
impl Patched for TurretData {
    type Patch = TurretPatch;

    fn from_patch(patch: TurretPatch) -> Result<Self, PatchError> {
        let turn_rate = patch.turn_rate.ok_or(PatchError::Missing("turn_rate"))?;
        let tolerance_deg = patch
            .tolerance_deg
            .ok_or(PatchError::Missing("tolerance_deg"))?;
        let slot = patch.slot.ok_or(PatchError::Missing("slot"))?;
        Self::new(turn_rate, tolerance_deg, slot).ok_or(PatchError::Refused(
            "a turret's turn rate is positive, and its tolerance from 0 to 180 degrees",
        ))
    }
}

impl<'de> Deserialize<'de> for TurretData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
}

impl DataSection for TurretData {
    const TABLE: DataTable = DataTable::new("A unit type's `turret`");
}

impl Section for TurretData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "turret";
    const STATUS: Status = Status::Runs(ApiVersion::FIRST);
}
