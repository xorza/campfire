//! Receiver-side history support for diff-replicated confirmed state.

use core::marker::PhantomData;

use crate::component_codec::ComponentCodec;
use crate::confirmed_history::{self, ConfirmedHistory, ReceivedValue};
use crate::history_component::HistoryComponent;
use crate::history_state::HistoryState;
use bevy_ecs::error::BevyError;
use bevy_ecs::prelude::{Entity, Query, ResMut};
use bevy_replicon::shared::replication::deferred_entity::DeferredEntity;
use bevy_replicon::shared::replication::diff::ComponentDelta;
use bevy_replicon::shared::replication::diff::{
    DiffBuffer, Diffable as RepliconDiffable, diff_index::DiffIndex,
};
use bevy_replicon::shared::replication::registry::ctx::WriteCtx;
use bevy_replicon::shared::replication::storage::{EntityStorageCtx, ReplicationStorage};
use bytes::Bytes;
use campfire_net_core::NetTick;
use campfire_net_core::Ticked;
use tracing::debug;

/// Why a diff message could not be queued or materialized.
#[derive(Debug)]
pub enum HistoryDiffError {
    /// A message holds more diffs than a cursor range spans.
    TooManyDiffs { diff_count: usize },
    /// Applying a diff to its base value failed.
    ApplyDiff(BevyError),
}

impl From<BevyError> for HistoryDiffError {
    fn from(error: BevyError) -> Self {
        Self::ApplyDiff(error)
    }
}

impl core::fmt::Display for HistoryDiffError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::TooManyDiffs { diff_count } => {
                write!(
                    f,
                    "a diff message holds {diff_count} diffs, more than a cursor range spans"
                )
            }
            Self::ApplyDiff(error) => write!(f, "a diff did not apply to its base: {error}"),
        }
    }
}

impl core::error::Error for HistoryDiffError {}

#[derive(Debug, Clone)]
struct PendingDiffMessage<Diff> {
    tick: NetTick,
    first_diff_index: DiffIndex,
    /// The cursor after the last diff, checked once when the message is queued.
    cursor: DiffIndex,
    diffs: Vec<Diff>,
}

impl<Diff> PendingDiffMessage<Diff> {
    /// The message for `diffs` from `first_diff_index`, whose count a cursor range must span.
    fn new(
        tick: NetTick,
        first_diff_index: DiffIndex,
        diffs: Vec<Diff>,
    ) -> Result<Self, HistoryDiffError> {
        let offset = diffs
            .len()
            .checked_sub(1)
            .expect("empty diff messages are not queued");
        let offset = u16::try_from(offset).map_err(|_| HistoryDiffError::TooManyDiffs {
            diff_count: diffs.len(),
        })?;
        Ok(Self {
            tick,
            first_diff_index,
            cursor: first_diff_index + offset,
            diffs,
        })
    }

    fn base_cursor(&self) -> DiffIndex {
        self.first_diff_index - 1
    }
}

/// Tracks diff cursors for materialized [`ConfirmedHistory`] entries.
///
/// This is deliberately separate from Replicon's live `DiffBuffer`: prediction
/// and interpolation need to reconstruct historical confirmed values from diff
/// messages without advancing a single live base cursor past older ticks that
/// can still arrive later.
///
/// Cursor model:
/// - A diff cursor is the state after a diff has been applied.
///   `None` is the pre-diff state before diff index `0`.
/// - `base_cursor`/`base_tick` are the retained floor. They identify the
///   oldest cursor that can still be used as a base, and the lowest retained
///   confirmed-history tick whose value represents that cursor.
/// - `diff_ticks` maps newer diff cursors to the confirmed-history tick that
///   stores the materialized state for that cursor.
///
/// Invariant:
/// - If `base_tick` is present, it is the lowest retained tick in this receiver.
/// - All keys in `diff_ticks` are strictly newer than `base_cursor`.
/// - Every tick in `diff_ticks` is greater than or equal to `base_tick`.
/// - After pruning to a processed confirmed tick, the newest cursor at or
///   before the prune tick is promoted to `base_cursor`, and `base_tick` is
///   moved to the prune tick so the base can be fetched from `ConfirmedHistory`.
///
/// Lifecycle:
/// 1. The prediction/interpolation `write_fn` receives a
///    [`ComponentDelta`](bevy_replicon::shared::replication::diff::ComponentDelta).
/// 2. Snapshots are written directly into [`ConfirmedHistory`] and their cursor
///    is recorded with [`Self::record_cursor`].
/// 3. Diff messages are added with [`Self::queue_diffs`]. They may be
///    buffered if their base cursor is not materialized yet.
/// 4. The `write_fn` calls [`Self::take_ready_update`] in a loop. Each ready
///    message fetches its base value from [`ConfirmedHistory`], applies the
///    diffs, records the new cursor, and returns the materialized value
///    for insertion into [`ConfirmedHistory`].
/// 5. After prediction/interpolation prunes [`ConfirmedHistory`],
///    [`Self::clear_before_tick`] promotes the newest usable cursor at its
///    retained floor and drains older cursor/pending state.
#[derive(Debug, Clone)]
pub struct HistoryDiffReceiver<C: RepliconDiffable> {
    base_cursor: Option<DiffIndex>,
    base_tick: Option<NetTick>,
    /// Materialized cursors newer than `base_cursor`, paired with the confirmed-history tick that
    /// stores each cursor's state.
    ///
    /// [`ConfirmedHistory`] can provide the effective authoritative state at a completed
    /// confirmed tick, but it does not associate that state with a diff cursor. These mappings are
    /// retained so a pending diff index can locate the history value for its specific base cursor.
    ///
    /// Entries are appended when their cursor is materialized and existing cursor ticks can be
    /// updated in place, so this vector is not ordered by either cursor or tick. Cursor lookup,
    /// tick-boundary lookup, and pruning therefore scan the retained entries.
    diff_ticks: Vec<(DiffIndex, NetTick)>,
    /// Received diff ranges that have not yet been materialized into confirmed history.
    ///
    /// Replicon acknowledges a mutation when its bytes are received, before applying it, and
    /// applies buffered mutation messages from newest to oldest. The server can therefore send a
    /// newer range based on an acknowledged cursor whose older message has not yet run this
    /// receiver's history callback. The newer range waits here until that callback records its
    /// base cursor, usually later in the same receive pass.
    ///
    /// The cursor stream is linear. If a later overlapping range or snapshot materializes a newer
    /// cursor first, [`Self::record_cursor`] removes any pending ranges that it supersedes.
    ///
    /// Entries are kept in arrival order, but are not processed strictly in that order:
    /// [`Self::take_ready_update`] scans for the first range whose base cursor is available. In
    /// normal use the caller drains ready ranges immediately after queuing, so retained entries are
    /// waiting on a missing base cursor.
    pending: Vec<PendingDiffMessage<C::Diff>>,
    marker: PhantomData<fn() -> C>,
}

impl<C: RepliconDiffable> Default for HistoryDiffReceiver<C> {
    fn default() -> Self {
        Self {
            base_cursor: None,
            base_tick: None,
            diff_ticks: Vec::default(),
            pending: Vec::default(),
            marker: PhantomData,
        }
    }
}

impl<C: HistoryComponent + RepliconDiffable> HistoryDiffReceiver<C> {
    /// Writes a diff message of `C` into its confirmed history: a snapshot's value, then each queued
    /// diff that the history now gives a base for, in order. `before_write` sees each value before
    /// the history records it, so a caller can act on it.
    ///
    /// A message that the entity's confirmed ticks do not map is dropped. An entity without a confirmed
    /// history gets one: its values are written there and read back for the diffs they make ready,
    /// as the entity's own component goes on only when the message's changes are flushed.
    pub fn receive(
        ctx: &mut WriteCtx<'_>,
        entity: &mut DeferredEntity<'_>,
        message: &mut Bytes,
        mut before_write: impl FnMut(&mut DeferredEntity<'_>, ReceivedValue<'_, C>),
    ) -> Result<(), BevyError> {
        let diff: ComponentDelta<C> = ComponentCodec::read_replicon(message)?;
        let Some(tick) = confirmed_history::replicated_message_tick(entity, ctx.message_tick)
        else {
            debug!(
                message_tick = ?ctx.message_tick,
                component = core::any::type_name::<C>(),
                "dropping a diff that no confirmed tick maps"
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
                ctx.get_or_default::<Self>()
                    .record_cursor(tick, Some(index));
                before_write(
                    entity,
                    ReceivedValue {
                        tick,
                        value: &component,
                        snapshot: true,
                    },
                );
                ConfirmedHistory::write(entity, &mut new_history, tick, component);
            }
            ComponentDelta::Diffs { index, diffs } => {
                ctx.get_or_default::<Self>()
                    .queue_diff(tick, index, diffs)?;
            }
        }
        while let Some(Ticked { tick, value }) = {
            let receiver = ctx.get_or_default::<Self>();
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
            ConfirmedHistory::write(entity, &mut new_history, tick, value);
        }
        if let Some(history) = new_history {
            entity.insert(history);
        }
        Ok(())
    }
}

impl<C: RepliconDiffable> HistoryDiffReceiver<C> {
    /// Gives `entity` a receiver whose base is the last diff Replicon applied to it, confirmed at
    /// `tick`: the diffs after it then resolve against the value that `tick` holds. An entity
    /// that has a receiver keeps it, and one with no applied diff gets none.
    pub fn seed(storage: &mut ReplicationStorage, entity: Entity, tick: NetTick) {
        let Some(cursor) = storage
            .get::<DiffBuffer<C>>(entity)
            .and_then(DiffBuffer::<C>::last_applied)
        else {
            return;
        };
        storage.get_or_init::<Self>(entity, || {
            let mut receiver = Self::default();
            receiver.record_cursor(tick, Some(cursor));
            receiver
        });
    }

    /// Records that confirmed history contains the state for `cursor` at `tick`.
    ///
    /// `cursor` is the diff cursor after the confirmed state was produced:
    /// - `None` for a snapshot with no diff base, i.e. the state before diff `0`.
    /// - `Some(i)` for a snapshot or diff message whose value is the state
    ///   after diff batch `i`.
    ///
    /// Cursors older than the retained base are ignored. Recording `None`
    /// resets the receiver to a full-snapshot base for a new diff stream.
    pub(crate) fn record_cursor(&mut self, tick: NetTick, cursor: Option<DiffIndex>) {
        if let Some(cursor) = cursor {
            if let Some(base_cursor) = self.base_cursor {
                if cursor != base_cursor && !cursor.is_newer(base_cursor) {
                    return;
                }
                if cursor == base_cursor {
                    if self.base_tick.is_none_or(|base_tick| tick >= base_tick) {
                        self.base_tick = Some(tick);
                    }
                    self.diff_ticks.retain(|(index, _)| index.is_newer(cursor));
                    self.pending
                        .retain(|pending| pending.cursor.is_newer(cursor));
                    return;
                }
            }
            if let Some((_, cursor_tick)) = self
                .diff_ticks
                .iter_mut()
                .find(|(known_cursor, _)| *known_cursor == cursor)
            {
                *cursor_tick = tick;
            } else {
                self.diff_ticks.push((cursor, tick));
            }
            self.pending
                .retain(|pending| pending.cursor.is_newer(cursor));
        } else {
            if self.base_tick.is_some_and(|base_tick| tick < base_tick) {
                return;
            }
            self.base_cursor = None;
            self.base_tick = Some(tick);
            self.diff_ticks.clear();
            self.pending.clear();
        }
    }

    /// The confirmed tick corresponding to `cursor`.
    pub fn tick_for_cursor(&self, cursor: Option<DiffIndex>) -> Option<NetTick> {
        if cursor == self.base_cursor {
            return self.base_tick;
        }
        let cursor = cursor?;
        self.diff_ticks
            .iter()
            .find_map(|(known_cursor, tick)| (*known_cursor == cursor).then_some(*tick))
    }

    /// The newest retained cursor recorded at or before `tick`.
    fn cursor_at_or_before(&self, tick: NetTick) -> Option<Ticked<Option<DiffIndex>>> {
        self.base_tick
            .map(|base_tick| Ticked::new(base_tick, self.base_cursor))
            .into_iter()
            .chain(
                self.diff_ticks
                    .iter()
                    .map(|(cursor, cursor_tick)| Ticked::new(*cursor_tick, Some(*cursor))),
            )
            .filter(|cursor| cursor.tick <= tick)
            .max_by_key(|cursor| cursor.tick)
    }

    /// Drops cursor and pending-diff state older than `tick`, the oldest value the confirmed
    /// history keeps.
    ///
    /// A pending range older than `tick` goes: its value would land below the kept history. A
    /// pending range at or after `tick` keeps the receiver's state until it materializes or is
    /// superseded, so that the base it waits for is not pruned. Otherwise the newest cursor at
    /// or before `tick` is promoted to the retained base. This keeps one usable base for future
    /// diff messages without storing per-unchanged-tick cursor entries. When `base_is_present`
    /// is false, no cursor is promoted because the component is removed at the retained history
    /// floor.
    pub(crate) fn clear_before_tick(&mut self, tick: NetTick, base_is_present: bool) {
        self.pending.retain(|pending| pending.tick >= tick);
        if self.has_pending_diffs() {
            return;
        }
        if self.base_tick.is_some_and(|base_tick| tick < base_tick) {
            return;
        }

        let cursor_at_cut = self.cursor_at_or_before(tick).map(|cursor| cursor.value);
        if base_is_present && let Some(cursor) = cursor_at_cut {
            self.base_cursor = cursor;
            self.base_tick = Some(tick);
        } else {
            self.base_cursor = None;
            self.base_tick = None;
        }
        if let Some(base_cursor) = self.base_cursor {
            self.diff_ticks
                .retain(|(cursor, _)| cursor.is_newer(base_cursor));
        }
        self.diff_ticks
            .retain(|(_, cursor_tick)| *cursor_tick >= tick);
    }

    /// Returns true while one or more diff messages are waiting for a base
    /// cursor to be materialized in confirmed history.
    pub const fn has_pending_diffs(&self) -> bool {
        !self.pending.is_empty()
    }

    /// The earliest unresolved diff tick that is not newer than `limit`.
    ///
    /// A pending diff at `P` makes the effective authoritative state unknown at every tick from `P`
    /// onward. A rollback confirmed tick `C >= P` is therefore unusable until the pending range is
    /// materialized or superseded. [`Self::record_cursor`] removes superseded ranges, so every
    /// entry considered here still constrains confirmed tick selection.
    pub fn earliest_pending_tick_at_or_before(&self, limit: NetTick) -> Option<NetTick> {
        self.pending
            .iter()
            .map(|pending| pending.tick)
            .filter(|tick| *tick <= limit)
            .min()
    }

    /// Returns true when a diff message at or before `tick` is waiting for its base cursor to be
    /// materialized in confirmed history.
    ///
    /// While this is true, the effective authoritative value at `tick` is not known yet. A
    /// completed confirmed tick must therefore not add a `SameAsPrecedent` history entry at `tick`:
    /// that marker would resolve past the missing diff to an older, potentially incorrect value.
    /// Pending ranges superseded by a later materialized cursor are removed by
    /// [`Self::record_cursor`], so they do not keep later confirmed ticks unresolved.
    pub fn has_pending_diff_at_or_before(&self, tick: NetTick) -> bool {
        self.earliest_pending_tick_at_or_before(tick).is_some()
    }

    /// Queues a diff message for historical materialization.
    ///
    /// `queue_diffs` only records the diff message. It does not apply the
    /// diffs immediately; callers should invoke [`Self::take_ready_update`]
    /// afterwards, usually in a loop.
    ///
    /// The message is ignored when its final cursor is at or before the retained
    /// base cursor, or when the exact message is already known/pending. Otherwise
    /// it is buffered until its `base_cursor` is available in this receiver.
    ///
    /// The method does not look up `ConfirmedHistory`; readiness is checked by
    /// [`Self::take_ready_update`], which is why out-of-order diff ranges can
    /// be queued before their base state arrives.
    pub(crate) fn queue_diffs(
        &mut self,
        tick: NetTick,
        first_diff_index: DiffIndex,
        diffs: Vec<C::Diff>,
    ) -> Result<(), HistoryDiffError> {
        if diffs.is_empty() {
            return Ok(());
        }
        let mut pending = PendingDiffMessage::new(tick, first_diff_index, diffs)?;
        let cursor = pending.cursor;
        if self.is_retired_cursor(cursor) || self.tick_for_cursor(Some(cursor)).is_some() {
            return Ok(());
        }

        if let Some(base_cursor) = self.base_cursor
            && base_cursor.is_newer(pending.base_cursor())
        {
            let retained_first_diff = base_cursor + 1;
            let drop_count = retained_first_diff.distance_after(pending.first_diff_index) as usize;
            if drop_count >= pending.diffs.len() {
                return Ok(());
            }
            pending.diffs.drain(0..drop_count);
            pending.first_diff_index = retained_first_diff;
        }

        if self.tick_for_cursor(Some(cursor)).is_some()
            || self.pending.iter().any(|queued| {
                queued.tick == pending.tick
                    && queued.first_diff_index == pending.first_diff_index
                    && queued.cursor == cursor
            })
        {
            return Ok(());
        }
        self.pending.push(pending);
        Ok(())
    }

    /// Queues a Replicon diff message encoded as the final cursor plus all
    /// diffs needed to reach it.
    pub(crate) fn queue_diff(
        &mut self,
        tick: NetTick,
        index: DiffIndex,
        diffs: Vec<C::Diff>,
    ) -> Result<(), HistoryDiffError> {
        if diffs.is_empty() {
            return Ok(());
        }
        let offset = diffs.len() - 1;
        let offset = u16::try_from(offset).map_err(|_| HistoryDiffError::TooManyDiffs {
            diff_count: diffs.len(),
        })?;
        // DiffIndex arithmetic wraps, so this also handles ranges that cross
        // u16::MAX, e.g. final index 3 with 10 diffs starts at 65530.
        let first_diff_index = index - offset;
        self.queue_diffs(tick, first_diff_index, diffs)
    }

    /// Attempts to materialize one queued diff message from `history`.
    ///
    /// A diff message is ready when the receiver has a tick for its base cursor
    /// and `ConfirmedHistory` can provide the corresponding base value.
    pub(crate) fn take_ready_update(
        &mut self,
        history: &ConfirmedHistory<C>,
    ) -> Result<Option<Ticked<C>>, HistoryDiffError>
    where
        C: Clone,
    {
        let Some((pending_index, mut value)) =
            self.pending.iter().enumerate().find_map(|(i, pending)| {
                let base_tick = self.tick_for_cursor(Some(pending.base_cursor()))?;
                if base_tick > pending.tick || history.has_removal_between(base_tick, pending.tick)
                {
                    return None;
                }
                history
                    .get_state_at_or_before(base_tick)
                    .and_then(HistoryState::value)
                    .cloned()
                    .map(|value| (i, value))
            })
        else {
            return Ok(None);
        };
        let pending = self.pending.remove(pending_index);
        for diff in &pending.diffs {
            value.apply_diff(diff)?;
        }
        self.record_cursor(pending.tick, Some(pending.cursor));
        Ok(Some(Ticked::new(pending.tick, value)))
    }

    fn is_retired_cursor(&self, cursor: DiffIndex) -> bool {
        self.base_cursor
            .is_some_and(|base_cursor| cursor == base_cursor || !cursor.is_newer(base_cursor))
    }

    /// Keep diff cursor history aligned with the oldest value retained in [`ConfirmedHistory`]
    /// (11-net-stack/replication.md, Each frame prunes every diff receiver to its history's front).
    ///
    /// Prediction and interpolation hold their value-retention policies. This system, the
    /// receiver's one pruner, runs afterwards and promotes the matching cursor to the receiver's
    /// retained base. Pending diffs keep both their cursor state and historical base until they
    /// materialize, are superseded, or fall behind the kept history.
    pub(crate) fn prune(
        storage: Option<ResMut<'_, ReplicationStorage>>,
        query: Query<'_, '_, (Entity, &ConfirmedHistory<C>)>,
    ) {
        let Some(mut storage) = storage else {
            return;
        };
        for (entity, history) in query.iter() {
            let Some(front) = history.get_nth_state(0) else {
                continue;
            };
            let Some(receiver) = storage.get_mut::<Self>(entity) else {
                continue;
            };
            receiver.clear_before_tick(front.tick, front.value.value().is_some());
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use bevy_replicon::shared::replication::diff::{
        Diffable as RepliconDiffable, diff_index::DiffIndex,
    };
    use campfire_net_core::{NetTick, Ticked};

    use crate::confirmed_history::ConfirmedHistory;
    use crate::history_diff_receiver::{HistoryDiffError, HistoryDiffReceiver};

    /// The bridge's writes, for another crate's test that stands in for it.
    impl<C: RepliconDiffable> HistoryDiffReceiver<C> {
        /// Records `cursor` at `tick`, as the bridge's `record_cursor` does.
        pub fn bridge_record_cursor(&mut self, tick: NetTick, cursor: Option<DiffIndex>) {
            self.record_cursor(tick, cursor);
        }

        /// Queues a message of `diffs` from `first_diff_index`, as the bridge's `queue_diffs`
        /// does.
        pub fn bridge_queue_diffs(
            &mut self,
            tick: NetTick,
            first_diff_index: DiffIndex,
            diffs: Vec<C::Diff>,
        ) -> Result<(), HistoryDiffError> {
            self.queue_diffs(tick, first_diff_index, diffs)
        }

        /// Queues the diffs ending at `index`, as the bridge's `queue_diff` does.
        pub fn bridge_queue_diff(
            &mut self,
            tick: NetTick,
            index: DiffIndex,
            diffs: Vec<C::Diff>,
        ) -> Result<(), HistoryDiffError> {
            self.queue_diff(tick, index, diffs)
        }

        /// The next update `history` lets apply, as the bridge's `take_ready_update` takes it.
        pub fn bridge_take_ready_update(
            &mut self,
            history: &ConfirmedHistory<C>,
        ) -> Result<Option<Ticked<C>>, HistoryDiffError>
        where
            C: Clone,
        {
            self.take_ready_update(history)
        }
    }
}

#[cfg(test)]
mod tests;
