use crate::mode::offer::Offer;
use crate::units::action_id::ActionId;
use crate::values::names::Names;

/// The entries of the mode's loadout packages, loaded: each one's id and ability, by offer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LoadoutSetup {
    ids: Names<Offer>,
    abilities: Vec<ActionId>,
}

impl LoadoutSetup {
    /// Adds the entry `id` of `action` after the others; no other entry has its id, which the
    /// load checked.
    pub(crate) fn push(&mut self, id: &str, action: ActionId) {
        self.ids.add(id);
        self.abilities.push(action);
    }

    /// The same entries, sorted by id: each entry's offer is its place in that order.
    pub(crate) fn sorted(&self) -> LoadoutSetup {
        let mut order: Vec<Offer> = self.ids.iter().map(|(offer, _)| offer).collect();
        order.sort_by(|&a, &b| self.id(a).cmp(self.id(b)));
        let mut sorted = LoadoutSetup::default();
        for offer in order {
            sorted.push(self.id(offer), self.ability(offer));
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
        let mut loadout = LoadoutSetup::default();
        loadout.push("haste", haste);
        loadout.push("arc", arc);
        loadout.push("blink", blink);
        let sorted = loadout.sorted();
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
