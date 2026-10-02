use crate::mode::choice_data::Offers;
use crate::mode::mode_setup::LoadoutSetup;
use crate::mode::offer::Offer;
use crate::units::action_id::ActionId;
use crate::values::name_list::NameList;

/// The avatars and loadout entries a mode's choices offer. Package data, not state.
#[derive(Debug)]
pub(crate) struct Roster {
    /// In the order of the mode's dependencies.
    avatars: NameList,
    /// Sorted by id.
    loadout: LoadoutSetup,
}

impl Roster {
    pub(crate) fn new(avatars: NameList, loadout: &LoadoutSetup) -> Roster {
        Roster {
            avatars,
            loadout: loadout.sorted(),
        }
    }

    /// The value `id` among `offers`.
    pub(crate) fn offer(&self, offers: Offers, id: &str) -> Option<Offer> {
        let at = match offers {
            Offers::Avatars => self.avatars.named(id)?,
            Offers::Loadout => self.loadout.sorted_place(id)?,
        };
        Some(Offer::new(at))
    }

    /// Whether `offer` is one of `offers`.
    pub(crate) const fn holds(&self, offers: Offers, offer: Offer) -> bool {
        let count = match offers {
            Offers::Avatars => self.avatars.len(),
            Offers::Loadout => self.loadout.len(),
        };
        offer.index() < count
    }

    /// The id of `offer` among `offers`.
    pub(crate) fn id(&self, offers: Offers, offer: Offer) -> &str {
        match offers {
            Offers::Avatars => self.avatars.get(offer.index()).expect("an offered avatar"),
            Offers::Loadout => self.loadout.id(offer.index()),
        }
    }

    /// The ability of the loadout entry `id`.
    pub(crate) fn loadout_ability(&self, id: &str) -> Option<ActionId> {
        let offer = self.offer(Offers::Loadout, id)?;
        Some(self.loadout.ability(offer.index()))
    }
}
