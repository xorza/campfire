use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_state_types::{DataKind, Replication, SentOnChange};
use campfire_values::{ModifierId, Rank};
use serde::{Deserialize, Serialize};

use crate::Stats;

/// The modifiers a player holds for the units it owns, as an RTS's upgrades, on its entity: each
/// once, in order.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerModifiers(Vec<ModifierId>);

impl PlayerModifiers {
    /// Adds `modifier`, unless it holds it already.
    pub fn add(&mut self, modifier: ModifierId) {
        if let Err(at) = self.0.binary_search(&modifier) {
            self.0.insert(at, modifier);
        }
    }

    /// Removes `modifier`, if it holds it.
    pub fn remove(&mut self, modifier: ModifierId) {
        if let Ok(at) = self.0.binary_search(&modifier) {
            self.0.remove(at);
        }
    }

    /// Whether it holds `modifier`.
    pub fn holds(&self, modifier: ModifierId) -> bool {
        self.0.binary_search(&modifier).is_ok()
    }

    /// The modifiers it holds, in order.
    pub fn iter(&self) -> impl Iterator<Item = ModifierId> + '_ {
        self.0.iter().copied()
    }
}

impl SimComponent for PlayerModifiers {
    const NAME: &'static str = "stats.player_modifiers";
    type Host = Player;

    // A modifier the book lacks would be read past its place, and one that reads a param no
    // action gives would fail as its units take it; `add` and `holds` find a modifier only in
    // order, each once.
    fn check(&self, world: &World, _: Entity) -> bool {
        self.0.is_sorted_by(|a, b| a < b)
            && self
                .0
                .iter()
                .all(|&modifier| Stats::modifier_has_way(world, modifier, None, Rank::FIRST))
    }
}

impl Replication for PlayerModifiers {
    const KIND: DataKind = DataKind::PlayerModifiers;
    type Sending = SentOnChange;
}
