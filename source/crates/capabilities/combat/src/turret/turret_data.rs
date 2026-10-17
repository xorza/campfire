use campfire_common::DeclaredName;
use campfire_math::Num;
use campfire_script::{DataSection, DataTable};
use serde::de::Error;
use serde::{Deserialize, Deserializer};

/// A unit type's `turret` section (04-capabilities/combat.md, A unit type may have a turret):
/// its attacks of the slot kind `slot` turn with a turret, `turn_rate` degrees a second, and
/// start within `tolerance_deg` of their target's bearing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurretData {
    pub turn_rate: Num,
    pub tolerance_deg: Num,
    pub slot: DeclaredName,
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
        if fields.turn_rate <= Num::ZERO {
            return Err(D::Error::custom("a turret's turn rate is positive"));
        }
        if !(Num::ZERO..=Num::int(180)).contains(&fields.tolerance_deg) {
            return Err(D::Error::custom(
                "a turret's tolerance is from 0 to 180 degrees",
            ));
        }
        Ok(Self {
            turn_rate: fields.turn_rate,
            tolerance_deg: fields.tolerance_deg,
            slot: fields.slot,
        })
    }
}

impl DataSection for TurretData {
    const TABLE: DataTable = DataTable::Turret;
}
