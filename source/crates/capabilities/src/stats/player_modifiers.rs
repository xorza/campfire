use bevy_ecs::resource::Resource;
use campfire_math::PlayerSlot;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::units::modifier_id::ModifierId;

/// The modifiers each player holds for the units it owns, as an RTS's upgrades: by player, then
/// by modifier, each once.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerModifiers(Vec<PlayerModifier>);

/// A modifier a player holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PlayerModifier {
    pub player: PlayerSlot,
    pub modifier: ModifierId,
}

impl PlayerModifiers {
    /// Gives `player` the modifier `modifier`, unless it holds it already.
    pub(crate) fn add(&mut self, held: PlayerModifier) {
        if let Err(at) = self.0.binary_search(&held) {
            self.0.insert(at, held);
        }
    }

    /// The modifiers `player` holds, in order.
    pub(crate) fn of(&self, player: PlayerSlot) -> impl Iterator<Item = ModifierId> + '_ {
        let first = self.0.partition_point(|held| held.player < player);
        self.0[first..]
            .iter()
            .take_while(move |held| held.player == player)
            .map(|held| held.modifier)
    }
}

impl SimResource for PlayerModifiers {
    const NAME: &'static str = "stats.player_modifiers";
}
