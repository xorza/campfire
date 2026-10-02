use crate::mode::choice_data::Offers;
use crate::mode::mode_setup::LoadoutSetup;
use crate::mode::offer::Offer;
use crate::units::action_id::ActionId;

/// The avatars and loadout entries a mode's choices offer. Package data, not state.
#[derive(Debug)]
pub(crate) struct Roster {
    /// In the order of the mode's dependencies.
    avatars: Vec<String>,
    /// Sorted by id.
    loadout: Vec<LoadoutSetup>,
}

impl Roster {
    pub(crate) fn new(avatars: Vec<String>, mut loadout: Vec<LoadoutSetup>) -> Roster {
        loadout.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Roster { avatars, loadout }
    }

    /// The value `id` among `offers`.
    pub(crate) fn offer(&self, offers: Offers, id: &str) -> Option<Offer> {
        let at = match offers {
            Offers::Avatars => self.avatars.iter().position(|avatar| avatar == id)?,
            Offers::Loadout => self
                .loadout
                .binary_search_by(|entry| entry.id.as_str().cmp(id))
                .ok()?,
        };
        Some(Offer::new(at))
    }

    /// The id of `offer` among `offers`.
    pub(crate) fn id(&self, offers: Offers, offer: Offer) -> &str {
        match offers {
            Offers::Avatars => &self.avatars[offer.index()],
            Offers::Loadout => &self.loadout[offer.index()].id,
        }
    }

    /// The ability of the loadout entry `id`.
    pub(crate) fn loadout_ability(&self, id: &str) -> Option<ActionId> {
        let offer = self.offer(Offers::Loadout, id)?;
        Some(self.loadout[offer.index()].ability)
    }
}
