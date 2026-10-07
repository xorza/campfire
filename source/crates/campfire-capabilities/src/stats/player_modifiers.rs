use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_sim::SimResource;
use serde::{Deserialize, Serialize};

use crate::stats::modifier_book::ModifierBook;
use crate::units::modifier_id::ModifierId;
use crate::units::script_view::View;

/// The modifiers each player holds for the units it owns, as an RTS's upgrades: by player, then
/// by modifier, each once.
#[derive(Resource, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct PlayerModifiers(Vec<PlayerModifier>);

/// A modifier a player holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) struct PlayerModifier {
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

    /// Whether `player` holds `modifier`.
    pub(crate) fn holds(&self, player: PlayerSlot, modifier: ModifierId) -> bool {
        self.0
            .binary_search(&PlayerModifier { player, modifier })
            .is_ok()
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

    // A modifier the book lacks, or a player the session lacks, would be read past their places,
    // and one that reads a param no action gives would fail as its units take it; `add` and `of`
    // find a player's modifiers only in order, each once.
    fn check(&self, world: &World) -> bool {
        let view = world.get_non_send::<View>();
        self.0.is_sorted_by(|a, b| a < b)
            && self.0.iter().all(|held| {
                ModifierBook::has_way_in(world, held.modifier, None, 1)
                    && view.is_none_or(|view| view.has_player(held.player))
            })
    }
}
