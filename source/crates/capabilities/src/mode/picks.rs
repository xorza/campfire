use bevy_ecs::resource::Resource;
use campfire_sim::{PlayerSlot, SimResource};
use serde::{Deserialize, Serialize};

use crate::mode::hero_index::HeroIndex;
use crate::mode::spell_index::SpellIndex;

/// What each player chose, by slot: a hero, by its place among the mode's, and spells, by theirs.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Picks(pub(crate) Vec<Pick>);

/// One player's choices, and whether their hero spawned.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    pub hero: Option<HeroIndex>,
    pub spells: Vec<SpellIndex>,
    pub spawned: bool,
}

impl Picks {
    pub const fn get(&self) -> &[Pick] {
        self.0.as_slice()
    }

    /// The choices of player `slot`.
    pub fn of(&self, slot: PlayerSlot) -> &Pick {
        &self.0[slot.index()]
    }

    pub(crate) fn of_mut(&mut self, slot: PlayerSlot) -> &mut Pick {
        &mut self.0[slot.index()]
    }

    /// Whether a player other than `slot` chose `hero`.
    pub(crate) fn taken(&self, slot: PlayerSlot, hero: HeroIndex) -> bool {
        self.0
            .iter()
            .enumerate()
            .any(|(other, pick)| other != slot.index() && pick.hero == Some(hero))
    }
}

impl SimResource for Picks {
    const NAME: &'static str = "mode.picks";
}
