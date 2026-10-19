//! The receive path that prediction and interpolation share: the values of a component that
//! Replicon's messages write into its confirmed history.

use crate::component_codec::ComponentCodec;
use crate::confirmed_history::ConfirmedHistory;
use crate::history_diff_receiver::HistoryDiffReceiver;
use crate::replication_checkpoint_map::ReplicationCheckpointMap;
use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::error::Result;
use bevy_replicon::prelude::RepliconTick;
use bevy_replicon::shared::replication::deferred_entity::DeferredEntity;
use bevy_replicon::shared::replication::diff::{ComponentDelta, Diffable as RepliconDiffable};
use bevy_replicon::shared::replication::registry::ctx::WriteCtx;
use bevy_replicon::shared::replication::storage::EntityStorageCtx;
use bytes::Bytes;
use campfire_net_core::NetTick;
use campfire_net_core::Ticked;
use tracing::debug;

/// The authoritative tick of the replicated message of `message_tick`; one that no checkpoint
/// maps is counted as a fault of the peer's data.
pub fn replicated_message_tick(
    entity: &DeferredEntity<'_>,
    message_tick: RepliconTick,
) -> Option<NetTick> {
    entity
        .world()
        .resource::<ReplicationCheckpointMap>()
        .message_tick(message_tick)
}

/// One value a confirmed history takes from a replicated message.
#[derive(Debug)]
pub struct ReceivedValue<'a, C> {
    pub tick: NetTick,
    pub value: &'a C,
    /// Whether the message carried the value whole, as a snapshot, rather than a queued diff
    /// the message made ready.
    pub snapshot: bool,
}

/// Writes a diff message of `C` into its confirmed history: a snapshot's value, then each queued
/// diff that the history now gives a base for, in order. `before_write` sees each value before
/// the history records it, so a caller can act on it.
///
/// A message that the entity's checkpoints do not map is dropped. An entity without a confirmed
/// history gets one: its values are written there and read back for the diffs they make ready,
/// as the entity's own component goes on only when the message's changes are flushed.
pub fn receive_history_diff<
    C: Component<Mutability = Mutable> + Clone + PartialEq + RepliconDiffable,
>(
    ctx: &mut WriteCtx<'_>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
    mut before_write: impl FnMut(&mut DeferredEntity<'_>, ReceivedValue<'_, C>),
) -> Result<()> {
    let diff: ComponentDelta<C> = ComponentCodec::read_replicon(message)?;
    let Some(tick) = replicated_message_tick(entity, ctx.message_tick) else {
        debug!(
            message_tick = ?ctx.message_tick,
            component = core::any::type_name::<C>(),
            "dropping a diff that no checkpoint maps"
        );
        return Ok(());
    };
    let mut new_history = None;
    match diff {
        ComponentDelta::Snapshot {
            index,
            mut component,
        } => {
            C::map_entities(&mut component, ctx);
            ctx.get_or_default::<HistoryDiffReceiver<C>>()
                .record_cursor(tick, Some(index));
            before_write(
                entity,
                ReceivedValue {
                    tick,
                    value: &component,
                    snapshot: true,
                },
            );
            write(entity, &mut new_history, tick, component);
        }
        ComponentDelta::Diffs { index, diffs } => {
            ctx.get_or_default::<HistoryDiffReceiver<C>>()
                .queue_diff(tick, index, diffs)?;
        }
    }
    while let Some(Ticked { tick, value }) = {
        let receiver = ctx.get_or_default::<HistoryDiffReceiver<C>>();
        match new_history.as_ref() {
            Some(history) => receiver.take_ready_update(history)?,
            None => entity
                .get::<ConfirmedHistory<C>>()
                .map(|history| receiver.take_ready_update(history))
                .transpose()?
                .flatten(),
        }
    } {
        before_write(
            entity,
            ReceivedValue {
                tick,
                value: &value,
                snapshot: false,
            },
        );
        write(entity, &mut new_history, tick, value);
    }
    if let Some(history) = new_history {
        entity.insert(history);
    }
    Ok(())
}

/// Records `value` at `tick` in the entity's confirmed history, or in the one it gets.
fn write<C: Component<Mutability = Mutable> + PartialEq>(
    entity: &mut DeferredEntity<'_>,
    new_history: &mut Option<ConfirmedHistory<C>>,
    tick: NetTick,
    value: C,
) {
    if let Some(mut history) = entity.get_mut::<ConfirmedHistory<C>>() {
        history.insert_present(tick, value);
    } else {
        new_history
            .get_or_insert_with(ConfirmedHistory::default)
            .insert_present(tick, value);
    }
}
