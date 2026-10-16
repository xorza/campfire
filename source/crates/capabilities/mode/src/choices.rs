use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::choice_specs::{ChoiceSpec, ChoiceSpecs};
use crate::match_mode::MatchMode;
use crate::offer::Offer;
use crate::roster::Roster;

/// What each player chose: for each player slot, a row of each choice's values, in the order
/// of the mode's choices, `None` where the player has not chosen.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Choices(Vec<Option<Offer>>);

impl Choices {
    /// The choices of `players` players of `specs`, none chosen.
    pub(crate) fn empty(specs: &ChoiceSpecs, players: usize) -> Self {
        Self(vec![None; specs.width() * players])
    }

    /// Whether these choices hold a row of `specs` for each of `players` players, as `choose`
    /// makes them: each choice all chosen or none, its values each one of its offers in `roster`
    /// and no two alike, and a value of a unique choice no other player's.
    pub(crate) fn fits(&self, specs: &ChoiceSpecs, players: usize, roster: &Roster) -> bool {
        if Some(self.0.len()) != specs.width().checked_mul(players) {
            return false;
        }
        (0..players).all(|row| {
            specs.iter().all(|choice| {
                let values = &self.0[specs.run(row, choice)];
                let whole =
                    values.iter().all(Option::is_some) || values.iter().all(Option::is_none);
                let distinct = (1..values.len())
                    .all(|at| values[at].is_none() || !values[..at].contains(&values[at]));
                let slot = PlayerSlot::new(u32::try_from(row).expect("a player's row fits u32"));
                let offered = values.iter().flatten().all(|&offer| {
                    roster.holds(choice.offers, offer)
                        && !(choice.unique && self.taken(specs, slot, choice, offer))
                });
                whole && distinct && offered
            })
        })
    }

    /// What `slot` chose of `choice` of `specs`: all its values, in order, or `None` before the
    /// player chose.
    pub(crate) fn chosen<'a>(
        &'a self,
        specs: &ChoiceSpecs,
        slot: PlayerSlot,
        choice: &ChoiceSpec,
    ) -> Option<impl Iterator<Item = Offer> + 'a> {
        let values = &self.0[specs.run(slot.index(), choice)];
        let chose = values.iter().all(Option::is_some);
        chose.then(|| values.iter().flatten().copied())
    }

    /// Records `values`, as many as `choice` of `specs` takes, as what `slot` chose of it.
    pub(crate) fn choose(
        &mut self,
        specs: &ChoiceSpecs,
        slot: PlayerSlot,
        choice: &ChoiceSpec,
        values: &[Offer],
    ) {
        let run = specs.run(slot.index(), choice);
        debug_assert_eq!(run.len(), values.len(), "a choice's count of values");
        for (held, &value) in self.0[run].iter_mut().zip(values) {
            *held = Some(value);
        }
    }

    /// Whether a player other than `slot` chose `offer` of `choice` of `specs`.
    pub(crate) fn taken(
        &self,
        specs: &ChoiceSpecs,
        slot: PlayerSlot,
        choice: &ChoiceSpec,
        offer: Offer,
    ) -> bool {
        let rows = self.0.len().checked_div(specs.width()).unwrap_or(0);
        (0..rows)
            .filter(|&row| row != slot.index())
            .any(|row| self.0[specs.run(row, choice)].contains(&Some(offer)))
    }
}

impl SimResource for Choices {
    const NAME: &'static str = "mode.choices";

    // A row or a value past the mode's choices would be read past their places.
    fn check(&self, world: &World) -> bool {
        let book = world.non_send::<MatchMode>();
        let players = usize::try_from(book.teams.players()).expect("a player count fits usize");
        self.fits(&book.choices, players, &book.roster)
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::choices::Choices;
    use crate::offer::Offer;

    impl Choices {
        /// The choices of `offers`, one a row, as a snapshot may hold them.
        pub const fn of(offers: Vec<Option<Offer>>) -> Self {
            Self(offers)
        }

        /// The offer each row took, if one.
        pub fn offers(&self) -> &[Option<Offer>] {
            &self.0
        }

        /// The offer each row took, to change as a flawed snapshot holds them.
        pub fn offers_mut(&mut self) -> &mut [Option<Offer>] {
            &mut self.0
        }
    }
}
