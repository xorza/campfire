use campfire_values::{ActionId, NameId, Names};

use crate::choice_data::Offers;
use crate::choice_specs::ChoiceSpec;
use crate::loadout::Loadout;
use crate::loadout_setup::LoadoutSetup;
use crate::offer::Offer;

/// The avatars and loadout entries a mode's choices offer, beside the values a choice lists
/// itself. Package data, not state.
#[derive(Debug)]
pub(crate) struct Roster {
    /// In the order of the mode's dependencies.
    avatars: Names<Offer>,
    loadout: Loadout,
}

impl Roster {
    pub(crate) fn new(avatars: Names<Offer>, loadout: &LoadoutSetup) -> Self {
        Self {
            avatars,
            loadout: Loadout::sorted(loadout),
        }
    }

    /// The value `id` among what `choice` offers.
    pub(crate) fn offer_named(&self, choice: &ChoiceSpec, id: &str) -> Option<Offer> {
        match choice.offers {
            Offers::Avatars => self.avatars.named(id),
            Offers::Loadout => self.loadout.ids().named(id),
            Offers::Values => choice.values.named(id),
        }
    }

    /// How many values `choice` offers.
    const fn count(&self, choice: &ChoiceSpec) -> usize {
        match choice.offers {
            Offers::Avatars => self.avatars.len(),
            Offers::Loadout => self.loadout.len(),
            Offers::Values => choice.values.len(),
        }
    }

    /// Whether `offer` is one of what `choice` offers.
    pub(crate) fn holds(&self, choice: &ChoiceSpec, offer: Offer) -> bool {
        offer.index() < self.count(choice)
    }

    /// The ids of what `choice` offers, in order.
    pub(crate) fn ids<'a>(&'a self, choice: &'a ChoiceSpec) -> impl Iterator<Item = &'a str> {
        (0..self.count(choice)).map(move |at| self.id(choice, Offer::new(at)))
    }

    /// The id of `offer` among what `choice` offers.
    pub(crate) fn id<'a>(&'a self, choice: &'a ChoiceSpec, offer: Offer) -> &'a str {
        match choice.offers {
            Offers::Avatars => self.avatars.name(offer).expect("an offered avatar"),
            Offers::Loadout => self.loadout.id(offer),
            Offers::Values => choice.values.name(offer).expect("a listed value"),
        }
    }

    /// The ability of the loadout entry `id`.
    pub(crate) fn loadout_ability_named(&self, id: &str) -> Option<ActionId> {
        let offer = self.loadout.ids().named(id)?;
        Some(self.loadout.ability(offer))
    }
}
