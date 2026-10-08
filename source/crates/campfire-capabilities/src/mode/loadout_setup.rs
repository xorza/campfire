use crate::units::action_id::ActionId;
use crate::values::name_list::NameList;

/// The entries of the mode's loadout packages, loaded: each one's id and ability, by place.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LoadoutSetup {
    ids: NameList,
    abilities: Vec<ActionId>,
}

impl LoadoutSetup {
    /// Adds the entry `id` of `ability` after the others.
    pub(crate) fn push(&mut self, id: &str, ability: ActionId) {
        self.ids.push(id);
        self.abilities.push(ability);
    }

    /// The same entries, sorted by id.
    pub(crate) fn sorted(&self) -> LoadoutSetup {
        let mut order: Vec<usize> = (0..self.len()).collect();
        order.sort_by(|&a, &b| self.id(a).cmp(self.id(b)));
        let mut sorted = LoadoutSetup::default();
        for at in order {
            sorted.push(self.id(at), self.abilities[at]);
        }
        sorted
    }

    /// The place of the entry `id`, in entries sorted by id.
    pub(crate) fn sorted_place(&self, id: &str) -> Option<usize> {
        debug_assert!(
            self.ids.iter().is_sorted(),
            "a loadout searched by id is sorted"
        );
        self.ids.sorted_named(0..self.len(), id)
    }

    pub(crate) fn id(&self, at: usize) -> &str {
        self.ids.get(at).expect("an entry of the loadout")
    }

    pub(crate) fn ability(&self, at: usize) -> ActionId {
        self.abilities[at]
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
            .map(|at| (sorted.id(at), sorted.ability(at)))
            .collect();
        assert_eq!(entries, [("arc", arc), ("blink", blink), ("haste", haste)]);
        assert_eq!(
            ["arc", "blink", "haste", "bolt"].map(|id| sorted.sorted_place(id)),
            [Some(0), Some(1), Some(2), None]
        );
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "a loadout searched by id is sorted")]
    fn a_loadout_not_sorted_is_not_searched_by_id() {
        let mut loadout = LoadoutSetup::default();
        loadout.push("haste", ActionId::new(0));
        loadout.push("arc", ActionId::new(1));
        loadout.sorted_place("arc");
    }
}
