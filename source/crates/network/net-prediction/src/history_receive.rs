//! Writing received values and removals into predicted components' confirmed histories.
//!
//! The authoritative value is retained in confirmed history. State mismatch decisions are made
//! later by the full scan at a completed checkpoint: an entity-level confirmation does not mean
//! every predicted component on that entity was updated, so receive-time checks could miss an
//! unchanged component's mismatch on the same entity.
use crate::prediction_history::PredictionHistory;
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::RuleFns;
use bevy_replicon::shared::replication::deferred_entity::DeferredEntity;
use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;
use bevy_replicon::shared::replication::registry::ctx::{RemoveCtx, WriteCtx};
use bevy_utils::prelude::DebugName;
use bytes::Bytes;
use campfire_net_core::NetTick;
use campfire_net_interpolation::HistoryComponent;
use campfire_net_replication::ConfirmedHistory;
use campfire_net_replication::HistoryState;
use campfire_net_replication::history_write::{self, ReceivedValue};
use campfire_net_timeline::LocalTimeline;
use tracing::{debug, trace};

/// What a first authoritative value of a component does on an entity without it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InitialValue {
    /// It goes in the confirmed history alone.
    HistoryOnly,
    /// It goes live too, as `PredictedSend` asks: a fresh entity materializes normally, and one
    /// predicted already keeps its history-only value. Replicon batches the live component and
    /// history insertions, so role-marker observers see the complete initial state.
    Materialize,
}

/// Instead of writing into a component directly, it writes data into [`ConfirmedHistory<C>`].
pub(crate) fn write_history<C: HistoryComponent>(
    ctx: &mut WriteCtx<'_>,
    rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
) -> Result<()> {
    write_whole(ctx, rule_fns, entity, message, InitialValue::HistoryOnly)
}

/// Writes a first authoritative value to both the live component and confirmed history.
pub(crate) fn write_initial_live_and_history<C: HistoryComponent>(
    ctx: &mut WriteCtx<'_>,
    rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
) -> Result<()> {
    write_whole(ctx, rule_fns, entity, message, InitialValue::Materialize)
}

fn write_whole<C: HistoryComponent>(
    ctx: &mut WriteCtx<'_>,
    rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
    initial: InitialValue,
) -> Result<()> {
    let component: C = rule_fns.deserialize(ctx, message)?;
    let Some(tick) = history_write::message_tick(entity, ctx.message_tick) else {
        debug!(
            message_tick = ?ctx.message_tick,
            "dropping a predicted value that no checkpoint maps"
        );
        return Ok(());
    };
    let materialized = materialize(entity, initial, &component);
    if !materialized {
        seed_unpredicted_insert::<C>(entity, tick);
    }
    record(entity, tick, HistoryState::Updated(component));
    Ok(())
}

pub(crate) fn write_history_diff<C: HistoryComponent + RepliconDiffable>(
    ctx: &mut WriteCtx<'_>,
    _rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
) -> Result<()> {
    write_diff::<C>(ctx, entity, message, InitialValue::HistoryOnly)
}

pub(crate) fn write_initial_live_and_history_diff<C: HistoryComponent + RepliconDiffable>(
    ctx: &mut WriteCtx<'_>,
    _rule_fns: &RuleFns<C>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
) -> Result<()> {
    write_diff::<C>(ctx, entity, message, InitialValue::Materialize)
}

/// A diff message's values go in through the receive path that interpolation shares: the
/// snapshot's, then each queued diff it makes ready. The entity's own components go on only
/// when the message's changes are flushed, so one materialized or seeded value is the message's
/// last.
fn write_diff<C: HistoryComponent + RepliconDiffable>(
    ctx: &mut WriteCtx<'_>,
    entity: &mut DeferredEntity<'_>,
    message: &mut Bytes,
    initial: InitialValue,
) -> Result<()> {
    let mut settled = false;
    history_write::receive_diff::<C>(
        ctx,
        entity,
        message,
        |entity,
         ReceivedValue {
             tick,
             value,
             snapshot,
         }| {
            trace!(
                target: "campfire_net_debug::prediction",
                kind = "confirmed_history_update",
                entity = ?entity.id(),
                component = ?DebugName::type_name::<C>(),
                confirmed_tick = tick.0,
                value = ?value,
                "recorded confirmed value in confirmed history"
            );
            if settled {
                return;
            }
            settled = (snapshot && materialize(entity, initial, value))
                || seed_unpredicted_insert::<C>(entity, tick);
        },
    )
}

/// Removes component `C` and records the removal in history.
pub(crate) fn remove_history<C: HistoryComponent>(
    ctx: &mut RemoveCtx,
    entity: &mut DeferredEntity<'_>,
) {
    let Some(tick) = history_write::message_tick(entity, ctx.message_tick) else {
        debug!(
            message_tick = ?ctx.message_tick,
            "dropping a predicted removal that no checkpoint maps"
        );
        return;
    };
    record::<C>(entity, tick, HistoryState::Removed);
}

/// Puts `value` live on an entity that has neither it nor a history of it, when `initial` asks,
/// and returns whether it did.
fn materialize<C: HistoryComponent>(
    entity: &mut DeferredEntity<'_>,
    initial: InitialValue,
    value: &C,
) -> bool {
    let materialize = initial == InitialValue::Materialize
        && entity.get::<C>().is_none()
        && entity.get::<PredictionHistory<C>>().is_none()
        && entity.get::<ConfirmedHistory<C>>().is_none();
    if materialize {
        entity.insert(value.clone());
    }
    materialize
}

/// Seeds the prediction history of a component that the server inserted on an entity whose
/// client never predicted it, and returns whether it did.
///
/// Normally `PredictionHistory<C>` is added when live `C` is added. A confirmed insertion on a
/// predicted entity is instead stored in `ConfirmedHistory<C>`, so the `Add<C>` observer does not
/// run, while `prepare_rollback<C>` needs the history to include the entity. It is seeded with
/// the state the client predicted, absent, so a rollback reads the confirmed value and inserts
/// `C`, even when this update finds no mismatch, as another component may already have.
fn seed_unpredicted_insert<C: HistoryComponent>(
    entity: &mut DeferredEntity<'_>,
    confirmed_tick: NetTick,
) -> bool {
    if entity.get::<PredictionHistory<C>>().is_some() || entity.get::<C>().is_some() {
        return false;
    }
    let current_tick = entity.world().resource::<LocalTimeline>().tick();
    // PredictionHistory must remain ordered on the local timeline. A confirmed update can be
    // ahead of the client, so seeding at confirmed_tick could put a future entry before
    // subsequent local predictions. The earlier tick records the known absence without preventing
    // those predictions from being appended in chronological order.
    let seed_tick = confirmed_tick.min(current_tick);
    trace!(
        target: "campfire_net_debug::prediction",
        kind = "seed_prediction_history_for_unpredicted_insert",
        entity = ?entity.id(),
        component = ?DebugName::type_name::<C>(),
        confirmed_tick = confirmed_tick.0,
        current_tick = current_tick.0,
        seed_tick = seed_tick.0,
        "seeding PredictionHistory with predicted absence for a confirmed insert of a never-predicted component"
    );
    let mut history = PredictionHistory::<C>::default();
    history.add_state(seed_tick, HistoryState::Removed);
    entity.insert(history);
    true
}

/// Records `state` at `tick` in the entity's confirmed history, which goes on if it is not there.
fn record<C: HistoryComponent>(
    entity: &mut DeferredEntity<'_>,
    tick: NetTick,
    state: HistoryState<C>,
) {
    trace!(
        target: "campfire_net_debug::prediction",
        kind = "confirmed_history_update",
        entity = ?entity.id(),
        component = ?DebugName::type_name::<C>(),
        confirmed_tick = tick.0,
        value = ?state.value(),
        "recorded confirmed value in confirmed history"
    );
    if let Some(mut history) = entity.get_mut::<ConfirmedHistory<C>>() {
        history.insert(tick, state);
    } else {
        let mut history = ConfirmedHistory::<C>::default();
        history.insert(tick, state);
        entity.insert(history);
    }
}
