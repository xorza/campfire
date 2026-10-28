use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_values::PerkId;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::perk_book::PerkBook;

/// The perks a player has, each once, in order (04-capabilities/progression.md, A science is a perk
/// of a player). State, sent to its owner.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerPerks(Vec<PerkId>);

impl PlayerPerks {
    /// Whether it has `perk`.
    pub fn holds(&self, perk: PerkId) -> bool {
        self.0.binary_search(&perk).is_ok()
    }

    /// The perks it has, in order.
    pub fn get(&self) -> &[PerkId] {
        &self.0
    }

    /// Gives it `perk`, whose modifier its player then holds (04-capabilities/progression.md, A
    /// perk's modifier is a player modifier): whether it lacked the perk.
    pub(crate) fn add(&mut self, perk: PerkId) -> bool {
        let at = self.0.binary_search(&perk);
        if let Err(at) = at {
            self.0.insert(at, perk);
        }
        at.is_err()
    }
}

impl SimComponent for PlayerPerks {
    const NAME: &'static str = "progression.player_perks";
    type Host = Player;

    // Each a perk of the mode, once, in order, as `holds` finds them.
    fn check(&self, world: &World, _: Entity) -> bool {
        let book = world.resource::<PerkBook>();
        self.0.is_sorted_by(|a, b| a < b) && self.0.iter().all(|&perk| book.names().has(perk))
    }
}

impl Replication for PlayerPerks {
    const KIND: DataKind = DataKind::Progression;
    type Sending = SentOnChange;
}
