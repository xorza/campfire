use crate::abilities::ability_book::AbilityId;
use crate::mode::avatar_index::AvatarIndex;
use crate::mode::loadout_index::LoadoutIndex;
use crate::mode::mode_setup::{AvatarSetup, LoadoutSetup};

/// The avatars and loadout a mode's players choose from. Package data, not state.
#[derive(Debug)]
pub(crate) struct Roster {
    /// In the order of the mode's dependencies.
    avatars: Vec<AvatarSetup>,
    /// Sorted by id.
    loadout: Vec<LoadoutSetup>,
}

impl Roster {
    pub(crate) fn new(avatars: Vec<AvatarSetup>, mut loadout: Vec<LoadoutSetup>) -> Roster {
        loadout.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Roster { avatars, loadout }
    }

    pub(crate) fn avatar(&self, id: &str) -> Option<AvatarIndex> {
        let at = self.avatars.iter().position(|avatar| avatar.id == id)?;
        Some(AvatarIndex::new(at))
    }

    pub(crate) fn avatar_setup(&self, avatar: AvatarIndex) -> &AvatarSetup {
        &self.avatars[avatar.index()]
    }

    pub(crate) fn loadout(&self, id: &str) -> Option<LoadoutIndex> {
        let at = self
            .loadout
            .binary_search_by(|entry| entry.id.as_str().cmp(id))
            .ok()?;
        Some(LoadoutIndex::new(at))
    }

    pub(crate) fn loadout_ability(&self, entry: LoadoutIndex) -> AbilityId {
        self.loadout[entry.index()].ability
    }
}
