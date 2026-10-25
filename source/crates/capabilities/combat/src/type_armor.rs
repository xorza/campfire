use campfire_declarations::PerId;
use campfire_script::Filter;
use campfire_values::{TagSet, UnitType};

use crate::armor::Armor;

/// The armors of a unit type: the one its unit takes damage through, and those it takes while its
/// tags pass a test, the first that passes first (upgrades.md, Armor by tags).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeArmor {
    pub armor: Option<Armor>,
    pub armors: Box<[TaggedArmor]>,
}

/// An armor a unit takes damage through while its tags pass `when`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaggedArmor {
    pub when: Filter,
    pub armor: Armor,
}

impl TypeArmor {
    /// The armor a unit of `tags` takes damage through: the first of `armors` whose test its tags
    /// pass, else `armor`; none takes the whole.
    pub fn of(&self, tags: TagSet) -> Option<&Armor> {
        let tagged = self
            .armors
            .iter()
            .find(|tagged| tagged.when.selects_tags(tags));
        tagged.map(|tagged| &tagged.armor).or(self.armor.as_ref())
    }
}

impl PerId<UnitType> for TypeArmor {
    const SCRIPTS_ONLY: bool = false;
}
