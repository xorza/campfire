use std::collections::{BTreeMap, BTreeSet};

use campfire_common::DeclaredName;
use campfire_values::NameKind;

use crate::fact::Fact;

/// The names of each kind that no table of ids holds, as the packages' data declares them: what
/// the load checks a name of such a kind against. A fact the modules that own such a kind add to,
/// each its own kinds, so a kind of a module the match does not run has no set.
#[derive(Debug, Default)]
pub struct NameSets(BTreeMap<NameKind, BTreeSet<DeclaredName>>);

impl Fact for NameSets {}

impl NameSets {
    /// Adds `names`, of `kind`, whose set then holds whatever else the data names none.
    pub fn extend<'n>(
        &mut self,
        kind: NameKind,
        names: impl IntoIterator<Item = &'n DeclaredName>,
    ) {
        self.0
            .entry(kind)
            .or_default()
            .extend(names.into_iter().cloned());
    }

    /// Whether the data declares `name` of `kind`; none for a kind no module added a set of.
    pub fn has(&self, kind: NameKind, name: &str) -> Option<bool> {
        self.0.get(&kind).map(|names| names.contains(name))
    }
}

#[cfg(test)]
mod tests {
    use campfire_values::{ActionId, ItemId, Named, PerkId};

    use super::*;

    #[test]
    fn a_set_answers_its_kind_alone() {
        let mut sets = NameSets::default();
        let sword = DeclaredName::new("sword").unwrap();
        sets.extend(ItemId::NAMES, [&sword]);
        sets.extend(ActionId::NAMES, []);
        assert_eq!(sets.has(ItemId::NAMES, "sword"), Some(true));
        assert_eq!(sets.has(ItemId::NAMES, "shield"), Some(false));
        // A kind added with no name has a set, which holds none.
        assert_eq!(sets.has(ActionId::NAMES, "sword"), Some(false));
        assert_eq!(sets.has(PerkId::NAMES, "sword"), None);
    }
}
