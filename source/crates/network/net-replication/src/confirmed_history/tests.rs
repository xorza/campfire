use super::*;

#[derive(Clone, PartialEq, Debug)]
struct TestValue(f32);

/// An entry as the history resolves it: explicit, or an anchor and the tick of the explicit
/// entry it takes its state from.
#[derive(Debug, PartialEq)]
enum Entry {
    Explicit(HistoryState<TestValue>),
    Same(NetTick),
}

fn explicit(value: f32) -> Entry {
    Entry::Explicit(HistoryState::Updated(TestValue(value)))
}

fn removed() -> Entry {
    Entry::Explicit(HistoryState::Removed)
}

/// An anchor unchanged from the explicit entry at `source`.
fn same(source: u32) -> Entry {
    Entry::Same(NetTick(source))
}

fn entries(history: &ConfirmedHistory<TestValue>) -> Vec<(NetTick, Entry)> {
    history
        .entries
        .iter()
        .map(|entry| {
            let tick = entry.tick;
            let resolved = if let Some(index) = history.explicit.index_at(tick) {
                Entry::Explicit(history.explicit.get(index).unwrap().value.clone())
            } else {
                let source = history.explicit.index_before(tick).unwrap();
                Entry::Same(history.explicit.tick(source).unwrap())
            };
            (tick, resolved)
        })
        .collect()
}

fn effective_value_at(history: &ConfirmedHistory<TestValue>, tick: NetTick) -> Option<f32> {
    history
        .get_state_at_or_before(tick)
        .and_then(HistoryState::value)
        .map(|value| value.0)
}

#[test]
fn an_insert_takes_exact_samples_out_of_order() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(5), TestValue(5.0));
    history.insert_present(NetTick(1), TestValue(1.0));
    history.insert_removed(NetTick(3));

    assert_eq!(
        entries(&history),
        vec![
            (NetTick(1), explicit(1.0)),
            (NetTick(3), removed()),
            (NetTick(5), explicit(5.0)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(2)), Some(1.0));
    assert!(
        history
            .get_state_at_or_before(NetTick(3))
            .and_then(HistoryState::value)
            .is_none()
    );
    assert_eq!(effective_value_at(&history, NetTick(5)), Some(5.0));
}

/// Strict-before lookup excludes the entry at the requested tick.
#[test]
fn get_state_before_excludes_the_requested_tick() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    history.insert_removed(NetTick(3));
    history.insert_present(NetTick(5), TestValue(5.0));

    assert!(history.get_state_before(NetTick(1)).is_none());
    assert_eq!(
        history
            .get_state_before(NetTick(3))
            .and_then(HistoryState::value),
        Some(&TestValue(1.0))
    );
    assert!(matches!(
        history.get_state_before(NetTick(5)),
        Some(HistoryState::Removed)
    ));
}

#[test]
fn an_unchanged_mark_keeps_the_later_samples() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(2), TestValue(2.0));
    history.insert_present(NetTick(10), TestValue(10.0));

    assert!(history.add_unchanged(NetTick(5)));
    assert_eq!(
        entries(&history),
        vec![
            (NetTick(2), explicit(2.0)),
            (NetTick(5), same(2)),
            (NetTick(10), explicit(10.0)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(5)), Some(2.0));
    assert_eq!(effective_value_at(&history, NetTick(9)), Some(2.0));
    assert_eq!(effective_value_at(&history, NetTick(10)), Some(10.0));
}

#[test]
fn an_unchanged_mark_follows_a_late_insert_before_it() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    history.insert_present(NetTick(3), TestValue(2.0));
    assert!(history.add_unchanged(NetTick(7)));

    history.insert_present(NetTick(5), TestValue(3.0));

    assert_eq!(
        entries(&history),
        vec![
            (NetTick(1), explicit(1.0)),
            (NetTick(3), explicit(2.0)),
            (NetTick(5), explicit(3.0)),
            (NetTick(7), same(5)),
        ]
    );
    assert_eq!(
        effective_value_at(&history, NetTick(7)),
        Some(3.0),
        "the unchanged tick should resolve to the late C@5 update"
    );
}

#[test]
fn an_equal_value_after_an_unchanged_mark_is_stored_unchanged() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    history.insert_present(NetTick(3), TestValue(2.0));
    assert!(history.add_unchanged(NetTick(7)));

    history.insert_present(NetTick(9), TestValue(2.0));

    assert_eq!(
        entries(&history),
        vec![
            (NetTick(1), explicit(1.0)),
            (NetTick(3), explicit(2.0)),
            (NetTick(7), same(3)),
            (NetTick(9), same(3)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(9)), Some(2.0));
}

#[test]
fn a_different_value_after_an_unchanged_mark_is_stored_as_it_is() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    history.insert_present(NetTick(3), TestValue(2.0));
    assert!(history.add_unchanged(NetTick(7)));

    history.insert_present(NetTick(9), TestValue(3.0));

    assert_eq!(
        entries(&history),
        vec![
            (NetTick(1), explicit(1.0)),
            (NetTick(3), explicit(2.0)),
            (NetTick(7), same(3)),
            (NetTick(9), explicit(3.0)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(7)), Some(2.0));
    assert_eq!(effective_value_at(&history, NetTick(9)), Some(3.0));
}

#[test]
fn an_equal_value_out_of_order_is_stored_unchanged() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    history.insert_present(NetTick(3), TestValue(2.0));
    assert!(history.add_unchanged(NetTick(7)));

    history.insert_present(NetTick(2), TestValue(1.0));

    assert_eq!(
        entries(&history),
        vec![
            (NetTick(1), explicit(1.0)),
            (NetTick(2), same(1)),
            (NetTick(3), explicit(2.0)),
            (NetTick(7), same(3)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(2)), Some(1.0));
    assert_eq!(effective_value_at(&history, NetTick(7)), Some(2.0));
}

#[test]
fn a_pushed_unchanged_mark_is_the_newest_anchor() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(2), TestValue(2.0));

    assert_eq!(history.push_unchanged(NetTick(5)), Some(NetTick(2)));
    assert_eq!(history.push_unchanged(NetTick(8)), Some(NetTick(5)));
    assert_eq!(
        entries(&history),
        vec![
            (NetTick(2), explicit(2.0)),
            (NetTick(5), same(2)),
            (NetTick(8), same(2)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(8)), Some(2.0));
}

#[test]
fn an_insert_out_of_order_before_an_unchanged_anchor_keeps_its_value() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(2), TestValue(2.0));

    assert_eq!(history.push_unchanged(NetTick(5)), Some(NetTick(2)));

    history.insert_present(NetTick(1), TestValue(1.0));
    assert_eq!(
        entries(&history),
        vec![
            (NetTick(1), explicit(1.0)),
            (NetTick(2), explicit(2.0)),
            (NetTick(5), same(2)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(5)), Some(2.0));

    assert_eq!(history.push_unchanged(NetTick(8)), Some(NetTick(5)));
    assert_eq!(
        entries(&history),
        vec![
            (NetTick(1), explicit(1.0)),
            (NetTick(2), explicit(2.0)),
            (NetTick(5), same(2)),
            (NetTick(8), same(2)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(8)), Some(2.0));
}

#[test]
fn an_insert_out_of_order_before_an_unchanged_tick_changes_its_value() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(2), TestValue(2.0));

    assert_eq!(history.push_unchanged(NetTick(5)), Some(NetTick(2)));

    history.insert_present(NetTick(3), TestValue(3.0));
    assert_eq!(
        entries(&history),
        vec![
            (NetTick(2), explicit(2.0)),
            (NetTick(3), explicit(3.0)),
            (NetTick(5), same(3)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(5)), Some(3.0));

    assert_eq!(history.push_unchanged(NetTick(8)), Some(NetTick(5)));
    assert_eq!(
        entries(&history),
        vec![
            (NetTick(2), explicit(2.0)),
            (NetTick(3), explicit(3.0)),
            (NetTick(5), same(3)),
            (NetTick(8), same(3)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(8)), Some(3.0));
}

#[test]
fn an_insert_at_an_unchanged_tick_replaces_the_mark() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(2), TestValue(2.0));

    assert_eq!(history.push_unchanged(NetTick(5)), Some(NetTick(2)));

    history.insert_present(NetTick(5), TestValue(5.0));
    assert_eq!(
        entries(&history),
        vec![(NetTick(2), explicit(2.0)), (NetTick(5), explicit(5.0)),]
    );

    assert_eq!(history.push_unchanged(NetTick(8)), Some(NetTick(5)));
    assert_eq!(
        entries(&history),
        vec![
            (NetTick(2), explicit(2.0)),
            (NetTick(5), explicit(5.0)),
            (NetTick(8), same(5)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(8)), Some(5.0));
}

#[test]
fn a_clear_until_a_tick_writes_the_value_of_an_unchanged_front() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    assert!(history.add_unchanged(NetTick(3)));
    assert!(history.add_unchanged(NetTick(7)));

    history.clear_until_tick(NetTick(3));

    assert_eq!(
        entries(&history),
        // The front at 3 takes the state of 1, which 7 then resolves to.
        vec![(NetTick(3), explicit(1.0)), (NetTick(7), same(3))]
    );
    assert_eq!(effective_value_at(&history, NetTick(7)), Some(1.0));
}

#[test]
fn clear_until_tick_keeps_effective_state_when_all_entries_are_older() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    history.insert_present(NetTick(4), TestValue(4.0));

    history.clear_until_tick(NetTick(6));

    assert_eq!(entries(&history), vec![(NetTick(6), explicit(4.0))]);
    assert_eq!(effective_value_at(&history, NetTick(6)), Some(4.0));
    assert_eq!(effective_value_at(&history, NetTick(9)), Some(4.0));
}

/// A late value at 4 inside the run from 1 moves the source of 7 and 9, after it, to 4, and
/// leaves 3, before it, on 1; a later prune to 8 keeps 9 resolving to 4's value through the
/// front.
#[test]
fn a_late_value_moves_the_run_after_it() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    for tick in [3, 7, 9] {
        assert!(history.add_unchanged(NetTick(tick)));
    }
    history.insert_present(NetTick(4), TestValue(4.0));
    assert_eq!(
        entries(&history),
        vec![
            (NetTick(1), explicit(1.0)),
            (NetTick(3), same(1)),
            (NetTick(4), explicit(4.0)),
            (NetTick(7), same(4)),
            (NetTick(9), same(4)),
        ]
    );
    assert_eq!(effective_value_at(&history, NetTick(3)), Some(1.0));
    assert_eq!(effective_value_at(&history, NetTick(9)), Some(4.0));

    history.clear_until_tick(NetTick(8));
    assert_eq!(effective_value_at(&history, NetTick(8)), Some(4.0));
    assert_eq!(effective_value_at(&history, NetTick(9)), Some(4.0));
}

/// A shift by -3 saturates 1 and 2 at 0 together: the unchanged 2 keeps tick 0 and takes the
/// explicit state of 1, so the front stays explicit, and 5 moves to 2 with its source, 1
/// saturated to 0.
#[test]
fn a_shift_keeps_the_front_explicit() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    assert!(history.add_unchanged(NetTick(2)));
    assert!(history.add_unchanged(NetTick(5)));

    history.shift(-3);

    assert_eq!(
        entries(&history),
        vec![(NetTick(0), explicit(1.0)), (NetTick(2), same(0))]
    );
    assert_eq!(effective_value_at(&history, NetTick(2)), Some(1.0));
}

/// With a value at 1, a removal at 4, a value at 6 and an unchanged anchor at 8, a range holds
/// the removal only when it covers tick 4: its start is exclusive and its end inclusive, and the
/// anchor at 8 resolves to the value at 6.
#[test]
fn has_removal_between_finds_the_removals_in_its_range() {
    let mut history = ConfirmedHistory::<TestValue>::default();
    history.insert_present(NetTick(1), TestValue(1.0));
    history.insert_removed(NetTick(4));
    history.insert_present(NetTick(6), TestValue(6.0));
    assert!(history.add_unchanged(NetTick(8)));

    let ranges = [(0, 10), (3, 4), (4, 10), (1, 3), (5, 8), (8, 20)];
    assert_eq!(
        ranges
            .map(|(after, through)| history.has_removal_between(NetTick(after), NetTick(through))),
        [true, true, false, false, false, false]
    );
}
