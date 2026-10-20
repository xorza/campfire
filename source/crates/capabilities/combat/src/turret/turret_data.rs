use campfire_common::DeclaredName;
use campfire_declarations::{Section, SectionTable};
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use serde::de::Error;
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

/// Data is untrusted, so a turn rate that is not positive, or a tolerance outside 0 to 180
/// degrees, fails to read.
impl<'de> Deserialize<'de> for TurretData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Debug, Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Fields {
            turn_rate: Num,
            tolerance_deg: Num,
            slot: DeclaredName,
        }
        let fields = Fields::deserialize(deserializer)?;
        Self::new(fields.turn_rate, fields.tolerance_deg, fields.slot).ok_or_else(|| {
            D::Error::custom(
                "a turret's turn rate is positive, and its tolerance from 0 to 180 degrees",
            )
        })
    }
}

impl DataSection for TurretData {
    const TABLE: DataTable = DataTable::new("A unit type's `turret`");
}

impl Section for TurretData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "turret";
}
