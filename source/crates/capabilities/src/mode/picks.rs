use bevy_ecs::resource::Resource;
use campfire_math::PlayerSlot;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::mode::avatar_index::AvatarIndex;
use crate::mode::loadout_index::LoadoutIndex;

/// What each player chose, by slot: an avatar, by its place among the mode's, and loadout, by theirs.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Picks(pub(crate) Vec<Pick>);

/// One player's choices, and whether their avatar spawned.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pick {
    pub avatar: Option<AvatarIndex>,
    pub loadout: Vec<LoadoutIndex>,
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

    /// Whether a player other than `slot` chose `avatar`.
    pub(crate) fn taken(&self, slot: PlayerSlot, avatar: AvatarIndex) -> bool {
        self.0
            .iter()
            .enumerate()
            .any(|(other, pick)| other != slot.index() && pick.avatar == Some(avatar))
    }
}

impl SimResource for Picks {
    const NAME: &'static str = "mode.picks";
}
