use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_players::Player;
use campfire_sim::SimComponent;
use campfire_states::{Replication, SentOnChange};
use campfire_values::ActionId;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::action_book::ActionBook;
use crate::cooldown_holder::CooldownHolder;

/// The first tick each action of a player's cooldown may start again, on its entity, by action,
/// each once, in order: none held is ready (generals.md, A power's cooldown may be its player's).
/// State, sent to its owner.
#[derive(Component, Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerCooldowns(Vec<PlayerCooldown>);

/// The tick an action of its player's cooldown may start again.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlayerCooldown {
    pub action: ActionId,
    pub ready_at: Tick,
}

impl PlayerCooldowns {
    /// The first tick `action` may start again; the first of all for one held by none.
    pub fn ready_at(&self, action: ActionId) -> Tick {
        let at = self.0.binary_search_by_key(&action, |held| held.action);
        at.map_or(Tick::ZERO, |at| self.0[at].ready_at)
    }

    /// Puts `action` on cooldown until `ready_at`.
    pub(crate) fn cool_down(&mut self, action: ActionId, ready_at: Tick) {
        match self.0.binary_search_by_key(&action, |held| held.action) {
            Ok(at) => self.0[at].ready_at = ready_at,
            Err(at) => self.0.insert(at, PlayerCooldown { action, ready_at }),
        }
    }

    /// Whether any action of `book` keeps its cooldown on its player, as a player then holds
    /// these.
    pub(crate) fn any(book: &ActionBook) -> bool {
        book.iter()
            .any(|action| action.cooldown_holder == CooldownHolder::Player)
    }
}

impl SimComponent for PlayerCooldowns {
    const NAME: &'static str = "actions.player_cooldowns";
    type Host = Player;

    // Each an action of the match that keeps its cooldown on its player, once, in order.
    fn check(&self, world: &World, _: Entity) -> bool {
        let book = world.resource::<ActionBook>();
        self.0.is_sorted_by(|a, b| a.action < b.action)
            && self.0.iter().all(|held| {
                book.get(held.action)
                    .is_some_and(|action| action.cooldown_holder == CooldownHolder::Player)
            })
    }
}

impl Replication for PlayerCooldowns {
    const KIND: DataKind = DataKind::PlayerCooldowns;
    type Sending = SentOnChange;
}
