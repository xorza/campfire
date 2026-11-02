//! Seeding a confirmed history from the live component it replaces.
use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::diff::Diffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::HistoryDiffReceiver;
use campfire_net_replication::{LiveValue, Seed};

/// Initializes `ConfirmedHistory<C>` from the current replicated component.
pub(crate) fn insert_confirmed_history<C: HistoryComponent>(
    entity: Entity,
    commands: &mut Commands<'_, '_>,
) {
    commands.queue(move |world: &mut World| {
        ConfirmedHistory::<C>::seed_from_live(world, entity, LiveValue::Moves);
    });
}

/// Diff-aware variant of [`insert_confirmed_history`]: the seeded value is also the base of the
/// diffs after Replicon's last applied one.
pub(crate) fn insert_confirmed_history_diff<C: HistoryComponent + Diffable>(
    entity: Entity,
    commands: &mut Commands<'_, '_>,
) {
    commands.queue(move |world: &mut World| {
        let Seed::Seeded(tick) =
            ConfirmedHistory::<C>::seed_from_live(world, entity, LiveValue::Moves)
        else {
            return;
        };
        if let Some(mut storage) = world.get_resource_mut::<ReplicationStorage>() {
            HistoryDiffReceiver::<C>::seed(&mut storage, entity, tick);
        }
    });
}
