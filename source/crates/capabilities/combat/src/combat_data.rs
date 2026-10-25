use campfire_common::DeclaredName;
use campfire_declarations::{PatchError, Patched, Section, SectionTable};
use campfire_script::{DataSection, DataTable};
use campfire_values::TagFilterData;
use serde::{Deserialize, Deserializer, Serialize};

use crate::on_death::OnDeath;

/// A unit type's `combat` section: it holds the life pool, stays when it dies or despawns, and
/// takes damage through the mode's armor it names, or through the first of `armors` whose test
/// its tags pass, whole with none. Its weapons are actions in its slots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CombatData {
    pub on_death: OnDeath,
    pub armor: Option<DeclaredName>,
    pub armors: Vec<TaggedArmorData>,
}

/// An armor of the mode's a unit takes damage through while its tags pass `when`
/// (upgrades.md, Armor by tags).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaggedArmorData {
    pub when: TagFilterData,
    pub armor: DeclaredName,
}

/// A unit type's `combat` as a tool writes it in part, over the defaults it lies on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_death: Option<OnDeath>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub armor: Option<DeclaredName>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub armors: Option<Vec<TaggedArmorData>>,
}

impl Patched for CombatData {
    type Patch = CombatPatch;

    fn from_patch(patch: CombatPatch) -> Result<Self, PatchError> {
        Ok(Self {
            on_death: patch.on_death.unwrap_or_default(),
            armor: patch.armor,
            armors: patch.armors.unwrap_or_default(),
        })
    }
}

impl<'de> Deserialize<'de> for CombatData {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::read_patch(deserializer)
    }
}

impl DataSection for CombatData {
    const TABLE: DataTable = DataTable::new("A unit type's `combat`");
}

impl Section for CombatData {
    const TABLE: SectionTable = SectionTable::UnitType;
    const KEY: &'static str = "combat";
}
