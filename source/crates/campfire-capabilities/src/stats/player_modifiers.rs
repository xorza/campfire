use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::players::player::Player;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnChange;
use crate::stats::Stats;
use crate::units::modifier_id::ModifierId;
use crate::values::rank::Rank;

/// The modifiers a player holds for the units it owns, as an RTS's upgrades, on its entity: each
/// once, in order.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerModifiers(Vec<ModifierId>);

impl PlayerModifiers {
    /// Adds `modifier`, unless it holds it already.
    pub(crate) fn add(&mut self, modifier: ModifierId) {
        if let Err(at) = self.0.binary_search(&modifier) {
            self.0.insert(at, modifier);
        }
    }

    /// Whether it holds `modifier`.
    pub(crate) fn holds(&self, modifier: ModifierId) -> bool {
        self.0.binary_search(&modifier).is_ok()
    }

    /// The modifiers it holds, in order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = ModifierId> + '_ {
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
