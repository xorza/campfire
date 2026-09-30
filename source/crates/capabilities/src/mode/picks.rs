use bevy_ecs::resource::Resource;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

/// What each player chose, by slot: a hero, by its place among the mode's, and spells, by theirs.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Picks(pub(crate) Vec<Pick>);

/// One player's choices, and whether their hero spawned.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    pub hero: Option<u16>,
    pub spells: Vec<u16>,
    pub spawned: bool,
}

impl Picks {
    pub const fn get(&self) -> &[Pick] {
        self.0.as_slice()
    }

    /// Whether a player other than `slot` chose `hero`.
    pub(crate) fn taken(&self, slot: usize, hero: u16) -> bool {
        self.0
            .iter()
            .enumerate()
            .any(|(other, pick)| other != slot && pick.hero == Some(hero))
    }
}

impl SimResource for Picks {
    const NAME: &'static str = "mode.picks";
}
