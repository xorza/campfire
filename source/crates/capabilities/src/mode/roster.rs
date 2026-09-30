use crate::abilities::ability_book::AbilityId;
use crate::mode::hero_index::HeroIndex;
use crate::mode::mode_setup::{HeroSetup, SpellSetup};
use crate::mode::spell_index::SpellIndex;

/// The heroes and spells a mode's players choose from. Package data, not state.
#[derive(Debug)]
pub(crate) struct Roster {
    /// In the order of the mode's dependencies.
    heroes: Vec<HeroSetup>,
    /// Sorted by id.
    spells: Vec<SpellSetup>,
}

impl Roster {
    pub(crate) fn new(heroes: Vec<HeroSetup>, mut spells: Vec<SpellSetup>) -> Roster {
        spells.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Roster { heroes, spells }
    }

    pub(crate) fn hero(&self, id: &str) -> Option<HeroIndex> {
        let at = self.heroes.iter().position(|hero| hero.id == id)?;
        Some(HeroIndex::new(at))
    }

    pub(crate) fn hero_setup(&self, hero: HeroIndex) -> &HeroSetup {
        &self.heroes[hero.index()]
    }

    pub(crate) fn spell(&self, id: &str) -> Option<SpellIndex> {
        let at = self
            .spells
            .binary_search_by(|spell| spell.id.as_str().cmp(id))
            .ok()?;
        Some(SpellIndex::new(at))
    }

    pub(crate) fn spell_ability(&self, spell: SpellIndex) -> AbilityId {
        self.spells[spell.index()].ability
    }
}
