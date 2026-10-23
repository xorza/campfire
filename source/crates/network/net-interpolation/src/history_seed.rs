//! Seeding a confirmed history from the live component it replaces.
use bevy_ecs::prelude::*;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use bevy_utils::prelude::DebugName;
use campfire_net_core::NetTick;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::ConfirmedTickMap;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::HistoryDiffReceiver;
use tracing::debug;

/// What seeding an entity's confirmed history from its live component did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seed {
    /// The entity is gone, holds no live component, or already has a history.
    Skipped,
    /// The live value entered the history at the tick its confirmation resolves to.
    Seeded(NetTick),
    /// No confirmation of the entity resolves to a tick yet: the live component stays, and the
    /// next frame's pass seeds it again.
    Unresolved,
}

/// Moves the live `C` into a new `ConfirmedHistory<C>`, at the tick its value is confirmed at;
/// with no tick to put it at, it leaves both as they are, so the entity stays in the seeding pass
/// and keeps its value until a confirmation resolves.
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
    let Some(tick) = tick else {
        debug!(
            ?entity,
            component = ?DebugName::type_name::<C>(),
            "no confirmation of the entity resolves to a tick; its history is seeded again next frame"
        );
        return Seed::Unresolved;
    };
    world
        .entity_mut(entity)
        .insert(ConfirmedHistory::seed(tick, component))
        .remove::<C>();
    Seed::Seeded(tick)
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
