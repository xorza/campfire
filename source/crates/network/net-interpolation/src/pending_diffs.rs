use core::cell::OnceCell;

use bevy_ecs::prelude::*;
use bevy_replicon::shared::replication::diff::Diffable;
use bevy_replicon::shared::replication::storage::ReplicationStorage;
use campfire_net_core::NetTick;
use campfire_net_replication::HistoryComponent;
use campfire_net_replication::HistoryDiffReceiver;

use crate::history_seed::{insert_confirmed_history, insert_confirmed_history_diff};

/// How `C` replicates, as its confirmed history waits on it: the diffs a message brought before
/// their base hold the ticks they land at, and the seed of a diff replicated history is the base
/// of the diffs after it.
pub(crate) trait PendingDiffs<C: HistoryComponent> {
    /// Whether a diff of `entity` that waits for its base lands at or before a tick. The receiver
    /// is looked up at the first call only, since most entities find their state in their history.
    fn pending(
        storage: Option<&ReplicationStorage>,
        entity: Entity,
    ) -> impl Fn(NetTick) -> bool + '_;

    /// Queues the seed of `entity`'s confirmed history from its live `C`.
    fn seed(entity: Entity, commands: &mut Commands<'_, '_>);
}

/// Whole replicated values, none of which waits.
#[derive(Debug)]
pub(crate) struct WholeValues;

/// Replicated diffs.
#[derive(Debug)]
pub(crate) struct Diffs;

impl<C: HistoryComponent> PendingDiffs<C> for WholeValues {
    fn pending(_: Option<&ReplicationStorage>, _: Entity) -> impl Fn(NetTick) -> bool + '_ {
        |_| false
    }

    fn seed(entity: Entity, commands: &mut Commands<'_, '_>) {
        insert_confirmed_history::<C>(entity, commands);
    }
}

impl<C: HistoryComponent + Diffable> PendingDiffs<C> for Diffs {
    fn pending(
        storage: Option<&ReplicationStorage>,
        entity: Entity,
    ) -> impl Fn(NetTick) -> bool + '_ {
        let receiver = OnceCell::new();
        move |tick| {
            receiver
                .get_or_init(|| storage?.get::<HistoryDiffReceiver<C>>(entity))
                .is_some_and(|receiver| receiver.has_pending_diff_at_or_before(tick))
        }
    }

    fn seed(entity: Entity, commands: &mut Commands<'_, '_>) {
        insert_confirmed_history_diff::<C>(entity, commands);
    }
}
