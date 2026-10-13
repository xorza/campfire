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
pub struct Choices(pub Vec<Option<Offer>>);

impl Choices {
    /// The choices of `players` players of `specs`, none chosen.
    pub(crate) fn empty(specs: &ChoiceSpecs, players: usize) -> Choices {
        Choices(vec![None; specs.width() * players])
    }

    /// Whether these choices hold a row of `specs` for each of `players` players, each value
    /// one of its choice's offers in `roster`.
    pub(crate) fn fits(&self, specs: &ChoiceSpecs, players: usize, roster: &Roster) -> bool {
        if Some(self.0.len()) != specs.width().checked_mul(players) {
            return false;
        }
        (0..players).all(|row| {
            specs.iter().all(|choice| {
                self.0[specs.run(row, choice)]
                    .iter()
                    .flatten()
                    .all(|&offer| roster.holds(choice.offers, offer))
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
        let players = book.teams.players() as usize;
        self.fits(&book.choices, players, &book.roster)
    }
}
