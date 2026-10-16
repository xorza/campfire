use campfire_values::{ActionId, Names};

use crate::offer::Offer;

/// The entries of the mode's loadout packages, loaded: each one's id and ability, in the order
/// the load adds them. The roster's `Loadout` sorts them by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadoutSetup {
    ids: Names<Offer>,
    abilities: Vec<ActionId>,
}

impl LoadoutSetup {
    /// Adds the entry `id` of `action` after the others; no other entry has its id, which the
    /// load checked.
    pub fn push(&mut self, id: &str, action: ActionId) {
        self.ids.add(id);
        self.abilities.push(action);
    }

    /// Each entry's id and ability, in the order added.
    pub(crate) fn entries(&self) -> impl ExactSizeIterator<Item = (&str, ActionId)> {
        self.ids.texts().zip(self.abilities.iter().copied())
    }

    pub const fn len(&self) -> usize {
        self.abilities.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
