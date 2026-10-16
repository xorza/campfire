use std::collections::BTreeMap;
use std::num::NonZeroUsize;
use std::ops::Range;
use std::sync::Arc;

use campfire_values::{DeclaredName, NameId, Names};

use crate::choice_data::{ChoiceData, Offers};
use crate::choice_id::ChoiceId;

/// The mode's choices, by name, and where each player's values of each sit in `Choices`.
/// Package data, not state.
#[derive(Debug, Default)]
pub(crate) struct ChoiceSpecs {
    /// The choices' names, by id.
    names: Arc<Names<ChoiceId>>,
    choices: Vec<ChoiceSpec>,
    /// The values of all choices of one player.
    width: usize,
}

/// A choice: what it offers, whether its values are each one player's, and its run of values in
/// a player's row.
#[derive(Debug, Clone)]
pub(crate) struct ChoiceSpec {
    pub(crate) offers: Offers,
    pub(crate) unique: bool,
    run: Range<usize>,
}

impl ChoiceSpecs {
    pub(crate) fn new(choices: &BTreeMap<DeclaredName, ChoiceData>) -> Self {
        let mut width = 0;
        let names = Arc::new(choices.keys().map(DeclaredName::as_str).collect());
        let choices = choices
            .values()
            .map(|data| {
                let start = width;
                width += NonZeroUsize::from(data.count).get();
                ChoiceSpec {
                    offers: data.offers,
                    unique: data.unique,
                    run: start..width,
                }
            })
            .collect();
        Self {
            names,
            choices,
            width,
        }
    }

    /// The choices' names, by id.
    pub(crate) const fn names(&self) -> &Arc<Names<ChoiceId>> {
        &self.names
    }

    /// The choice `id`.
    pub(crate) fn get(&self, id: ChoiceId) -> &ChoiceSpec {
        &self.choices[id.index()]
    }

    /// Each choice, in the order of their names.
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, ChoiceSpec> {
        self.choices.iter()
    }

    /// The values of all choices of one player.
    pub(crate) const fn width(&self) -> usize {
        self.width
    }

    /// Where the values of the player in row `row`, its slot's index, for `choice` sit.
    pub(crate) const fn run(&self, row: usize, choice: &ChoiceSpec) -> Range<usize> {
        let start = row * self.width;
        start + choice.run.start..start + choice.run.end
    }
}

impl<'a> IntoIterator for &'a ChoiceSpecs {
    type Item = &'a ChoiceSpec;
    type IntoIter = std::slice::Iter<'a, ChoiceSpec>;

    fn into_iter(self) -> std::slice::Iter<'a, ChoiceSpec> {
        self.iter()
    }
}

impl ChoiceSpec {
    /// How many values a player chooses.
    pub(crate) const fn count(&self) -> usize {
        self.run.end - self.run.start
    }
}
