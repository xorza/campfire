use super::*;

use bevy_ecs::component::Component;
use bevy_ecs::error::Result;

use bevy_replicon::shared::replication::diff::Diffable as RepliconDiffable;

use serde::{Deserialize, Serialize};

#[derive(Component, Clone, Debug, Deserialize, PartialEq, Serialize)]
struct TestDiffValue(u32);

impl RepliconDiffable for TestDiffValue {
    type Diff = u32;

    fn apply_diff(&mut self, diff: &Self::Diff) -> Result<()> {
        self.0 = *diff;
        Ok(())
    }
}

fn idx(value: u16) -> DiffIndex {
    DiffIndex::new(value)
}

/// A diff message for `S3 -> S5` is buffered when the receiver has not
/// materialized `S3` yet.
///
/// On the wire this is `first_diff_index = 4` with batches `4` and `5`:
/// batch `4` transforms cursor `3` into cursor `4`, and batch `5`
/// transforms cursor `4` into cursor `5`.
#[test]
fn a_diff_range_waits_for_its_base_state() {
    let mut history = ConfirmedHistory::<TestDiffValue>::default();
    history.insert_present(NetTick(0), TestDiffValue(0));

    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));

    receiver
        .queue_diffs(NetTick(5), idx(4), vec![4, 5])
        .unwrap();

    assert_eq!(receiver.take_ready_update(&history).unwrap(), None);
    assert_eq!(receiver.pending.len(), 1);
    assert_eq!(receiver.pending[0].base_cursor(), idx(3));
    assert_eq!(receiver.tick_for_cursor(Some(idx(5))), None);
}

/// Once the receiver has pruned through `S3`, a later `S1 -> S3` diff
/// message is stale. Its final cursor is at the retained base, so it cannot
/// produce a newer confirmed history value and is ignored.
#[test]
fn a_diff_range_at_or_before_the_kept_base_is_ignored() {
    let mut history = ConfirmedHistory::<TestDiffValue>::default();
    history.insert_present(NetTick(0), TestDiffValue(0));
    history.insert_present(NetTick(3), TestDiffValue(3));

    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));
    receiver.record_cursor(NetTick(3), Some(idx(3)));
    receiver.clear_before_tick(NetTick(3), true);

    receiver
        .queue_diffs(NetTick(3), idx(1), vec![1, 2, 3])
        .unwrap();

    assert!(receiver.pending.is_empty());
    assert_eq!(receiver.take_ready_update(&history).unwrap(), None);
    assert_eq!(receiver.tick_for_cursor(Some(idx(3))), Some(NetTick(3)));
}

/// A newer `S3 -> S5` message can arrive before the older `S0 -> S3`
/// message. The receiver keeps the newer message pending, materializes
/// `S3` when the older message arrives, and then uses that newly inserted
/// confirmed history entry as the base for `S5`.
///
/// Here `S0` is represented by cursor `0`, so the older message is encoded
/// as `first_diff_index = 1` with batches `1`, `2`, and `3`.
#[test]
fn an_older_diff_range_writes_the_newer_one_it_waited_for() {
    let mut history = ConfirmedHistory::<TestDiffValue>::default();
    history.insert_present(NetTick(0), TestDiffValue(0));

    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));

    receiver
        .queue_diffs(NetTick(5), idx(4), vec![4, 5])
        .unwrap();
    assert_eq!(receiver.take_ready_update(&history).unwrap(), None);

    receiver
        .queue_diffs(NetTick(3), idx(1), vec![1, 2, 3])
        .unwrap();
    let Ticked { tick, value } = receiver.take_ready_update(&history).unwrap().unwrap();
    assert_eq!(tick, NetTick(3));
    assert_eq!(value, TestDiffValue(3));
    history.insert_present(tick, value);

    let Ticked { tick, value } = receiver.take_ready_update(&history).unwrap().unwrap();
    assert_eq!(tick, NetTick(5));
    assert_eq!(value, TestDiffValue(5));
    history.insert_present(tick, value);

    assert_eq!(
        history
            .get_state_at(NetTick(3))
            .and_then(HistoryState::value)
            .cloned(),
        Some(TestDiffValue(3))
    );
    assert_eq!(
        history
            .get_state_at(NetTick(5))
            .and_then(HistoryState::value)
            .cloned(),
        Some(TestDiffValue(5))
    );
    assert_eq!(receiver.tick_for_cursor(Some(idx(3))), Some(NetTick(3)));
    assert_eq!(receiver.tick_for_cursor(Some(idx(5))), Some(NetTick(5)));
}

/// A later message can use an older available base and overlap an earlier pending message. Once
/// the later message materializes its newer cursor, the earlier pending range is superseded.
#[test]
fn a_newer_range_that_overlaps_supersedes_an_older_pending_one() {
    let mut history = ConfirmedHistory::<TestDiffValue>::default();
    history.insert_present(NetTick(0), TestDiffValue(0));

    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));

    // S3 -> S5 cannot be applied because S3 is not materialized.
    receiver
        .queue_diffs(NetTick(5), idx(4), vec![4, 5])
        .unwrap();
    assert_eq!(receiver.take_ready_update(&history).unwrap(), None);

    // S0 -> S6 overlaps the pending range but can be applied from S0.
    receiver
        .queue_diffs(NetTick(6), idx(1), vec![1, 2, 3, 4, 5, 6])
        .unwrap();
    let Ticked { tick, value } = receiver.take_ready_update(&history).unwrap().unwrap();
    assert_eq!((tick, value), (NetTick(6), TestDiffValue(6)));

    assert!(receiver.pending.is_empty());
    assert_eq!(receiver.tick_for_cursor(Some(idx(6))), Some(NetTick(6)));
}

/// Receiver cleanup must not retire bases or discard pending diff messages
/// while a newer diff range is waiting for an older base. Otherwise an
/// unchanged history anchor can be carried forward and the later base
/// message can no longer materialize the missing historical state.
#[test]
fn a_pending_diff_range_holds_the_cleanup_until_it_is_written() {
    let mut history = ConfirmedHistory::<TestDiffValue>::default();
    history.insert_present(NetTick(0), TestDiffValue(0));

    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));

    receiver
        .queue_diffs(NetTick(5), idx(4), vec![4, 5])
        .unwrap();
    // The history keeps its value at tick 0, the floor the pruner passes.
    receiver.clear_before_tick(NetTick(0), true);

    assert_eq!(receiver.pending.len(), 1);
    assert_eq!(receiver.tick_for_cursor(Some(idx(0))), Some(NetTick(0)));

    receiver
        .queue_diffs(NetTick(3), idx(1), vec![1, 2, 3])
        .unwrap();
    let Ticked { tick, value } = receiver.take_ready_update(&history).unwrap().unwrap();
    assert_eq!((tick, value.clone()), (NetTick(3), TestDiffValue(3)));
    history.insert_present(tick, value);

    let Ticked { tick, value } = receiver.take_ready_update(&history).unwrap().unwrap();
    assert_eq!((tick, value), (NetTick(5), TestDiffValue(5)));
}

/// A completed confirmed tick beyond a pending diff cannot receive an unchanged anchor until the
/// older diff materializes. Once it does, the anchor resolves immediately to the final
/// authoritative value and cannot change later.
#[test]
fn an_older_pending_diff_holds_a_later_unchanged_anchor_until_it_is_written() {
    let mut history = ConfirmedHistory::<TestDiffValue>::default();
    history.insert_present(NetTick(0), TestDiffValue(0));

    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));

    receiver
        .queue_diffs(NetTick(5), idx(4), vec![4, 5])
        .unwrap();
    assert_eq!(receiver.take_ready_update(&history).unwrap(), None);
    assert_eq!(
        receiver.earliest_pending_tick_at_or_before(NetTick(6)),
        Some(NetTick(5))
    );
    assert_eq!(
        receiver.earliest_pending_tick_at_or_before(NetTick(4)),
        None
    );
    assert!(receiver.has_pending_diff_at_or_before(NetTick(5)));
    assert!(receiver.has_pending_diff_at_or_before(NetTick(6)));
    assert!(!receiver.has_pending_diff_at_or_before(NetTick(4)));
    assert!(history.get_state_at(NetTick(6)).is_none());

    receiver
        .queue_diffs(NetTick(3), idx(1), vec![1, 2, 3])
        .unwrap();
    while let Some(Ticked { tick, value }) = receiver.take_ready_update(&history).unwrap() {
        history.insert_present(tick, value);
    }

    assert!(!receiver.has_pending_diff_at_or_before(NetTick(6)));
    assert_eq!(history.push_unchanged(NetTick(6)), Some(NetTick(5)));
    assert_eq!(
        history
            .get_state_at(NetTick(6))
            .and_then(HistoryState::value)
            .cloned(),
        Some(TestDiffValue(5))
    );
}

/// If a late message overlaps the retained base cursor, the receiver can
/// apply the suffix of that message from the retained base instead of
/// waiting forever for a retired cursor.
#[test]
fn an_overlapping_diff_range_is_cut_to_the_kept_base() {
    let mut history = ConfirmedHistory::<TestDiffValue>::default();
    history.insert_present(NetTick(0), TestDiffValue(0));
    history.insert_present(NetTick(3), TestDiffValue(3));

    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));
    receiver.record_cursor(NetTick(3), Some(idx(3)));
    receiver.clear_before_tick(NetTick(3), true);

    receiver
        .queue_diffs(NetTick(5), idx(1), vec![1, 2, 3, 4, 5])
        .unwrap();

    assert_eq!(receiver.pending.len(), 1);
    assert_eq!(receiver.pending[0].first_diff_index, idx(4));
    assert_eq!(receiver.pending[0].base_cursor(), idx(3));

    let Ticked { tick, value } = receiver.take_ready_update(&history).unwrap().unwrap();
    assert_eq!((tick, value), (NetTick(5), TestDiffValue(5)));
    assert!(receiver.pending.is_empty());
    assert_eq!(receiver.tick_for_cursor(Some(idx(5))), Some(NetTick(5)));
}

/// Replicon's diff indexes wrap at u16::MAX. A message ending at cursor 3
/// with 10 diffs starts at cursor 65530 and uses 65529 as its base.
#[test]
fn a_queued_diff_takes_a_wrapped_diff_index() {
    let mut history = ConfirmedHistory::<TestDiffValue>::default();
    history.insert_present(NetTick(0), TestDiffValue(0));

    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(u16::MAX - 6)));

    receiver
        .queue_diff(NetTick(10), idx(3), vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
        .unwrap();

    let Ticked { tick, value } = receiver.take_ready_update(&history).unwrap().unwrap();
    assert_eq!((tick, value), (NetTick(10), TestDiffValue(10)));
    assert!(receiver.pending.is_empty());
    assert_eq!(receiver.tick_for_cursor(Some(idx(3))), Some(NetTick(10)));
}

/// Pruning promotes the newest known cursor at or before the processed tick
/// to the retained base. `base_tick` becomes the lowest retained tick, and
/// `diff_ticks` only keeps newer cursors that may still be useful.
#[test]
fn a_prune_makes_the_processed_tick_the_kept_base() {
    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));
    receiver.record_cursor(NetTick(3), Some(idx(3)));
    receiver.record_cursor(NetTick(5), Some(idx(5)));

    receiver.clear_before_tick(NetTick(3), true);

    assert_eq!(receiver.base_cursor, Some(idx(3)));
    assert_eq!(receiver.base_tick, Some(NetTick(3)));
    assert_eq!(receiver.tick_for_cursor(None), None);
    assert_eq!(receiver.tick_for_cursor(Some(idx(3))), Some(NetTick(3)));
    assert_eq!(receiver.tick_for_cursor(Some(idx(5))), Some(NetTick(5)));
    assert!(
        receiver
            .diff_ticks
            .iter()
            .all(|(_, tick)| *tick >= receiver.base_tick.unwrap())
    );
}

/// Cleanup can be called with an older completed tick after a newer
/// snapshot was written in the same frame. That must not wipe the retained
/// base cursor, or the next diff range will be unable to find its base.
#[test]
fn a_prune_older_than_the_kept_base_changes_nothing() {
    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(128), Some(idx(0)));
    receiver.clear_before_tick(NetTick(128), true);

    assert_eq!(
        (receiver.base_cursor, receiver.base_tick),
        (Some(idx(0)), Some(NetTick(128)))
    );

    receiver.clear_before_tick(NetTick(126), true);

    assert_eq!(
        (receiver.base_cursor, receiver.base_tick),
        (Some(idx(0)), Some(NetTick(128)))
    );
    assert_eq!(receiver.tick_for_cursor(Some(idx(0))), Some(NetTick(128)));
}

/// A pending range that falls behind the kept history can never land in it: the pruner drops
/// it and prunes, so that a range whose base is lost does not hold the receiver's state forever.
#[test]
fn a_pending_range_behind_the_floor_is_dropped() {
    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    receiver.record_cursor(NetTick(0), Some(idx(0)));
    receiver.record_cursor(NetTick(2), Some(idx(1)));
    receiver
        .queue_diffs(NetTick(5), idx(4), vec![4, 5])
        .unwrap();
    receiver.queue_diffs(NetTick(9), idx(8), vec![8]).unwrap();

    receiver.clear_before_tick(NetTick(6), true);
    assert_eq!(receiver.pending.len(), 1);
    assert_eq!(receiver.pending[0].tick, NetTick(9));
    assert_eq!(receiver.tick_for_cursor(Some(idx(0))), Some(NetTick(0)));

    receiver.clear_before_tick(NetTick(10), true);
    assert!(!receiver.has_pending_diffs());
    assert_eq!(receiver.tick_for_cursor(Some(idx(1))), Some(NetTick(10)));
}

/// A message with more diffs than a cursor range spans is refused with its count.
#[test]
fn a_message_of_too_many_diffs_is_refused() {
    let mut receiver = HistoryDiffReceiver::<TestDiffValue>::default();
    let diffs = vec![0; usize::from(u16::MAX) + 2];
    assert!(matches!(
        receiver.queue_diffs(NetTick(1), idx(0), diffs),
        Err(HistoryDiffError::TooManyDiffs { diff_count: 65_537 })
    ));
}

/// A seed bases the receiver on the last diff Replicon applied, at the confirmed tick: the diff
/// after it then resolves. An entity with no applied diff gets no receiver, and one with a
/// receiver keeps it.
#[test]
fn a_seed_bases_the_receiver_on_the_last_applied_diff() {
    let mut storage = ReplicationStorage::default();
    let entity = Entity::from_raw_u32(1).unwrap();
    let unapplied = Entity::from_raw_u32(2).unwrap();
    storage.insert(entity, DiffBuffer::<TestDiffValue>::default());
    storage
        .get_mut::<DiffBuffer<TestDiffValue>>(entity)
        .unwrap()
        .set_last_applied(idx(3));
    storage.insert(unapplied, DiffBuffer::<TestDiffValue>::default());

    HistoryDiffReceiver::<TestDiffValue>::seed(&mut storage, entity, NetTick(7));
    HistoryDiffReceiver::<TestDiffValue>::seed(&mut storage, unapplied, NetTick(7));
    HistoryDiffReceiver::<TestDiffValue>::seed(&mut storage, entity, NetTick(9));

    let receiver = storage
        .get::<HistoryDiffReceiver<TestDiffValue>>(entity)
        .unwrap();
    assert_eq!(receiver.tick_for_cursor(Some(idx(3))), Some(NetTick(7)));
    assert!(
        storage
            .get::<HistoryDiffReceiver<TestDiffValue>>(unapplied)
            .is_none()
    );

    let mut history = ConfirmedHistory::<TestDiffValue>::default();
    history.insert_present(NetTick(7), TestDiffValue(3));
    let mut receiver = storage
        .remove::<HistoryDiffReceiver<TestDiffValue>>(entity)
        .unwrap();
    receiver.queue_diffs(NetTick(8), idx(4), vec![4]).unwrap();
    assert_eq!(
        receiver.take_ready_update(&history).unwrap(),
        Some(Ticked::new(NetTick(8), TestDiffValue(4)))
    );
}
