//! Seeding a confirmed history from the live component it replaces.
use crate::history_component::HistoryComponent;

use bevy_ecs::prelude::*;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use bevy_utils::prelude::DebugName;
use campfire_net_core::NetTick;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::ConfirmedTickMap;
use campfire_net_replication::HistoryDiffReceiver;
use tracing::warn;

/// What seeding an entity's confirmed history from its live component did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seed {
    /// The entity is gone, holds no live component, or already has a history.
    Skipped,
    /// The live value entered the history at the tick its confirmation resolves to.
    Seeded(NetTick),
    /// No confirmation of the entity resolves to a tick: the history starts empty, and the next
    /// update fills it.
    Unresolved,
}

/// Moves the live `C` into a new `ConfirmedHistory<C>`, at the tick its value is confirmed at.
/// The history is inserted whatever the outcome, so the archetype leaves the seeding pass.
fn seed_confirmed_history<C: HistoryComponent>(world: &mut World, entity: Entity) -> Seed {
    let Ok(entity_ref) = world.get_entity(entity) else {
        return Seed::Skipped;
    };
    if entity_ref.contains::<ConfirmedHistory<C>>() {
        return Seed::Skipped;
    }
    let Some(component) = entity_ref.get::<C>().cloned() else {
        return Seed::Skipped;
    };
    let tick = entity_ref
        .get::<ConfirmHistory>()
        .and_then(|confirm_history| {
            world
                .resource::<ConfirmedTickMap>()
                .confirmed_state_tick(confirm_history)
        });
    let mut history = ConfirmedHistory::<C>::default();
    let seed = if let Some(tick) = tick {
        history.insert_present(tick, component);
        Seed::Seeded(tick)
    } else {
        warn!(
            ?entity,
            component = ?DebugName::type_name::<C>(),
            "no confirmation of the entity resolves to a tick; its interpolation history waits for the next update"
        );
        Seed::Unresolved
    };
    world.entity_mut(entity).insert(history).remove::<C>();
    seed
}

/// Initializes `ConfirmedHistory<C>` from the current replicated component.
pub(crate) fn insert_confirmed_history<C: HistoryComponent>(
    entity: Entity,
    commands: &mut Commands<'_, '_>,
) {
    commands.queue(move |world: &mut World| {
        seed_confirmed_history::<C>(world, entity);
    });
}

/// Diff-aware variant of [`insert_confirmed_history`]: the seeded value is also the base of the
/// diffs after Replicon's last applied one.
pub(crate) fn insert_confirmed_history_diff<C: HistoryComponent + RepliconDiffable>(
    entity: Entity,
    commands: &mut Commands<'_, '_>,
) {
    commands.queue(move |world: &mut World| {
        let Seed::Seeded(tick) = seed_confirmed_history::<C>(world, entity) else {
            return;
        };
        if let Some(mut storage) = world.get_resource_mut::<ReplicationStorage>() {
            HistoryDiffReceiver::<C>::seed(&mut storage, entity, tick);
        }
    });
}
