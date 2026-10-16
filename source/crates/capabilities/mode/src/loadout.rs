use campfire_values::{ActionId, NameId, Names};

use crate::loadout_setup::LoadoutSetup;
use crate::offer::Offer;

/// The loadout entries a choice offers, sorted by id: each entry's offer is its place in that
/// order. Package data, not state.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Loadout {
    ids: Names<Offer>,
    abilities: Vec<ActionId>,
}

impl Loadout {
    /// The entries of `setup`, sorted by id.
    pub(crate) fn sorted(setup: &LoadoutSetup) -> Self {
        let mut entries: Vec<(&str, ActionId)> = setup.entries().collect();
        entries.sort_unstable_by_key(|&(id, _)| id);
        let mut sorted = Self::default();
        sorted.abilities.reserve_exact(entries.len());
        for (id, ability) in entries {
            sorted.ids.add(id);
            sorted.abilities.push(ability);
        }
        sorted
    }

    /// The entries' ids.
    pub(crate) const fn ids(&self) -> &Names<Offer> {
        &self.ids
    }

    pub(crate) fn id(&self, offer: Offer) -> &str {
        self.ids.name(offer).expect("an entry of the loadout")
    }

    pub(crate) fn ability(&self, offer: Offer) -> ActionId {
        self.abilities[offer.index()]
    }

    pub(crate) const fn len(&self) -> usize {
        self.abilities.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sorted_loadout_keeps_each_id_with_its_ability() {
        let [arc, blink, haste] = [0, 1, 2].map(ActionId::new);
        let mut setup = LoadoutSetup::default();
        setup.push("haste", haste);
        setup.push("arc", arc);
        setup.push("blink", blink);
        let sorted = Loadout::sorted(&setup);
        let entries: Vec<_> = (0..sorted.len())
            .map(|at| (sorted.id(Offer::new(at)), sorted.ability(Offer::new(at))))
            .collect();
        assert_eq!(entries, [("arc", arc), ("blink", blink), ("haste", haste)]);
        let offer = |at| Some(Offer::new(at));
        assert_eq!(
            ["arc", "blink", "haste", "bolt"].map(|id| sorted.ids().named(id)),
            [offer(0), offer(1), offer(2), None]
        );
    }
}
