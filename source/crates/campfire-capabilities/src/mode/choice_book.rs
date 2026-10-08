use std::collections::BTreeMap;
use std::ops::Range;

use campfire_common::PlayerSlot;

use crate::mode::choice_data::{ChoiceData, Offers};
use crate::mode::choices::Choices;
use crate::mode::offer::Offer;
use crate::mode::roster::Roster;
use crate::values::declared_name::DeclaredName;

/// The mode's choices, by name, and where each player's values of each sit in `Choices`.
/// Package data, not state.
#[derive(Debug, Default)]
pub(crate) struct ChoiceBook {
    choices: Vec<Choice>,
    /// The values of all choices of one player.
    width: usize,
}

/// A choice: what it offers, whether its values are each one player's, and its run of values in
/// a player's row.
#[derive(Debug, Clone)]
pub(crate) struct Choice {
    name: DeclaredName,
    pub(crate) offers: Offers,
    pub(crate) unique: bool,
    run: Range<usize>,
}

impl ChoiceBook {
    pub(crate) fn new(choices: &BTreeMap<DeclaredName, ChoiceData>) -> ChoiceBook {
        let mut width = 0;
        let choices = choices
            .iter()
            .map(|(name, data)| {
                let start = width;
                width += usize::from(data.count.get());
                Choice {
                    name: name.clone(),
                    offers: data.offers,
                    unique: data.unique,
                    run: start..width,
                }
            })
            .collect();
        ChoiceBook { choices, width }
    }

    /// The choice `name`.
    pub(crate) fn named(&self, name: &str) -> Option<&Choice> {
        let at = self
            .choices
            .binary_search_by(|choice| choice.name.as_str().cmp(name))
            .ok()?;
        Some(&self.choices[at])
    }

    /// The choices of `players` players, none chosen.
    pub(crate) fn empty(&self, players: usize) -> Choices {
        Choices(vec![None; self.width * players])
    }

    /// Whether `choices` holds a row for each of `players` players, each value one of its
    /// choice's offers in `roster`.
    pub(crate) fn fits(&self, choices: &Choices, players: usize, roster: &Roster) -> bool {
        if Some(choices.0.len()) != self.width.checked_mul(players) {
            return false;
        }
        (0..players).all(|row| {
            self.choices.iter().all(|choice| {
                choices.0[self.run(row, choice)]
                    .iter()
                    .flatten()
                    .all(|&offer| roster.holds(choice.offers, offer))
            })
        })
    }

    /// What `slot` chose of `choice`: all its values, in order, or `None` before the player
    /// chose.
    pub(crate) fn chosen<'a>(
        &self,
        choices: &'a Choices,
        slot: PlayerSlot,
        choice: &Choice,
    ) -> Option<impl Iterator<Item = Offer> + 'a> {
        let values = &choices.0[self.run(slot.index(), choice)];
        let chose = values.iter().all(Option::is_some);
        chose.then(|| values.iter().flatten().copied())
    }

    /// Records `values`, as many as `choice` takes, as what `slot` chose of it.
    pub(crate) fn choose(
        &self,
        choices: &mut Choices,
        slot: PlayerSlot,
        choice: &Choice,
        values: &[Offer],
    ) {
        let run = self.run(slot.index(), choice);
        debug_assert_eq!(run.len(), values.len(), "a choice's count of values");
        for (held, &value) in choices.0[run].iter_mut().zip(values) {
            *held = Some(value);
        }
    }

    /// Whether a player other than `slot` chose `offer` of `choice`.
    pub(crate) fn taken(
        &self,
        choices: &Choices,
        slot: PlayerSlot,
        choice: &Choice,
        offer: Offer,
    ) -> bool {
        let rows = choices.0.len().checked_div(self.width).unwrap_or(0);
        (0..rows)
            .filter(|&row| row != slot.index())
            .any(|row| choices.0[self.run(row, choice)].contains(&Some(offer)))
    }

    /// Where the values of the player in row `row`, its slot's index, for `choice` sit.
    const fn run(&self, row: usize, choice: &Choice) -> Range<usize> {
        let start = row * self.width;
        start + choice.run.start..start + choice.run.end
    }
}

impl Choice {
    /// How many values a player chooses.
    pub(crate) const fn count(&self) -> usize {
        self.run.end - self.run.start
    }
}
