use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use campfire_net_core::NetTick;
use tracing::debug;

use crate::confirmed_history::ConfirmedHistory;
use crate::confirmed_tick_map::ConfirmedTickMap;
use crate::history_component::HistoryComponent;

/// What seeding an entity's confirmed history from its live component did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Seed {
    /// The entity is gone, holds no live component, or already has a history.
    Skipped,
    /// The live value entered the history at the tick its confirmation resolves to.
    Seeded(NetTick),
    /// No confirmation of the entity resolves to a tick yet: the caller seeds it again later.
    Unresolved,
}

/// What a seed does with the live component: interpolation shows the history in its place, and
/// prediction keeps simulating it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveValue {
    Moves,
    Stays,
}

impl<C: HistoryComponent> ConfirmedHistory<C> {
    /// Seeds `entity`'s confirmed history of `C` from its live value, at the tick its
    /// confirmation resolves to, the live value moving into it or staying as `live` says; with no
    /// tick to put it at, it leaves the entity as it is, for the caller to seed it again.
    pub fn seed_from_live(world: &mut World, entity: Entity, live: LiveValue) -> Seed {
        let Ok(entity_ref) = world.get_entity(entity) else {
            return Seed::Skipped;
        };
        if entity_ref.contains::<Self>() {
            return Seed::Skipped;
        }
        let Some(component) = entity_ref.get::<C>().cloned() else {
            return Seed::Skipped;
        };
        let confirm_history = entity_ref.get::<ConfirmHistory>();
        let ticks = world.get_resource::<ConfirmedTickMap>();
        let tick = confirm_history
            .zip(ticks)
            .and_then(|(confirm_history, ticks)| ticks.confirmed_state_tick(confirm_history));
        let Some(tick) = tick else {
            debug!(
                ?entity,
                component = core::any::type_name::<C>(),
                "no confirmation of the entity resolves to a tick; its history is seeded again"
            );
            return Seed::Unresolved;
        };
        let mut seeded = world.entity_mut(entity);
        seeded.insert(Self::seed(tick, component));
        if live == LiveValue::Moves {
            seeded.remove::<C>();
        }
        Seed::Seeded(tick)
    }
}
