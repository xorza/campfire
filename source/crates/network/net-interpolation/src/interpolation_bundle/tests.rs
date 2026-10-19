use super::*;

use bevy_ecs::prelude::Component;

#[derive(Component, Clone, Debug, PartialEq)]
struct Value(f32);

#[derive(Debug, Clone, Copy)]
enum Entry {
    Present(f32),
    Removed,
    Unchanged,
}

#[derive(Debug, PartialEq)]
struct SampledBracket {
    bracket: PresentHistoryBracket,
    start: (Value, Value),
    end: Option<(Value, Value)>,
}

#[derive(Debug)]
struct Case {
    name: &'static str,
    first: &'static [(u32, Entry)],
    second: &'static [(u32, Entry)],
    interpolation_tick: u32,
    expected: Option<SampledBracket>,
}

fn history(entries: &[(u32, Entry)]) -> ConfirmedHistory<Value> {
    let mut history = ConfirmedHistory::default();
    for &(tick, entry) in entries {
        match entry {
            Entry::Present(value) => history.insert_present(NetTick(tick), Value(value)),
            Entry::Removed => history.insert_removed(NetTick(tick)),
            Entry::Unchanged => {
                assert!(history.add_unchanged(NetTick(tick)));
            }
        }
    }
    history
}

fn sample_pair(
    first: &ConfirmedHistory<Value>,
    second: &ConfirmedHistory<Value>,
    interpolation_tick: NetTick,
) -> Option<SampledBracket> {
    let bracket = present_history_bracket(first, interpolation_tick)?
        .intersection(present_history_bracket(second, interpolation_tick)?);
    let start = (
        present_history_value_at(first, bracket.start_tick)?.clone(),
        present_history_value_at(second, bracket.start_tick)?.clone(),
    );
    let end = match bracket.end_tick {
        Some(end_tick) => Some((
            present_history_value_at(first, end_tick)?.clone(),
            present_history_value_at(second, end_tick)?.clone(),
        )),
        None => None,
    };
    Some(SampledBracket {
        bracket,
        start,
        end,
    })
}

/// Shared bundle brackets across mismatched, unchanged, removed, and open histories.
#[test]
fn a_bundle_brackets_each_shape_of_its_histories() {
    let cases = [
        Case {
            name: "different immediate start and end ticks",
            first: &[(10, Entry::Present(1.0)), (30, Entry::Present(3.0))],
            second: &[(15, Entry::Present(10.0)), (20, Entry::Present(20.0))],
            interpolation_tick: 17,
            expected: Some(SampledBracket {
                bracket: PresentHistoryBracket {
                    start_tick: NetTick(15),
                    end_tick: Some(NetTick(20)),
                },
                start: (Value(1.0), Value(10.0)),
                end: Some((Value(1.0), Value(20.0))),
            }),
        },
        Case {
            name: "one member has no future entry",
            first: &[(10, Entry::Present(0.0)), (20, Entry::Present(10.0))],
            second: &[(10, Entry::Present(7.0))],
            interpolation_tick: 15,
            expected: Some(SampledBracket {
                bracket: PresentHistoryBracket {
                    start_tick: NetTick(10),
                    end_tick: Some(NetTick(20)),
                },
                start: (Value(0.0), Value(7.0)),
                end: Some((Value(10.0), Value(7.0))),
            }),
        },
        Case {
            name: "removal is a constant present endpoint",
            first: &[(10, Entry::Present(0.0)), (20, Entry::Present(10.0))],
            second: &[(10, Entry::Present(3.0)), (20, Entry::Removed)],
            interpolation_tick: 15,
            expected: Some(SampledBracket {
                bracket: PresentHistoryBracket {
                    start_tick: NetTick(10),
                    end_tick: Some(NetTick(20)),
                },
                start: (Value(0.0), Value(3.0)),
                end: Some((Value(10.0), Value(3.0))),
            }),
        },
        Case {
            name: "both members have no future entry",
            first: &[(10, Entry::Present(2.0))],
            second: &[(5, Entry::Present(4.0))],
            interpolation_tick: 15,
            expected: Some(SampledBracket {
                bracket: PresentHistoryBracket {
                    start_tick: NetTick(10),
                    end_tick: None,
                },
                start: (Value(2.0), Value(4.0)),
                end: None,
            }),
        },
        Case {
            name: "member is removed at the interpolation tick",
            first: &[(10, Entry::Present(0.0)), (20, Entry::Present(10.0))],
            second: &[(10, Entry::Present(3.0)), (20, Entry::Removed)],
            interpolation_tick: 20,
            expected: None,
        },
        Case {
            name: "member is present again at its reinsertion tick",
            first: &[(10, Entry::Present(0.0)), (30, Entry::Present(20.0))],
            second: &[
                (10, Entry::Removed),
                (20, Entry::Present(4.0)),
                (30, Entry::Present(8.0)),
            ],
            interpolation_tick: 20,
            expected: Some(SampledBracket {
                bracket: PresentHistoryBracket {
                    start_tick: NetTick(20),
                    end_tick: Some(NetTick(30)),
                },
                start: (Value(0.0), Value(4.0)),
                end: Some((Value(20.0), Value(8.0))),
            }),
        },
        Case {
            name: "unchanged member carries across the other member's endpoint",
            first: &[(10, Entry::Present(3.0)), (20, Entry::Unchanged)],
            second: &[
                (10, Entry::Present(0.0)),
                (15, Entry::Present(10.0)),
                (20, Entry::Unchanged),
            ],
            interpolation_tick: 12,
            expected: Some(SampledBracket {
                bracket: PresentHistoryBracket {
                    start_tick: NetTick(10),
                    end_tick: Some(NetTick(15)),
                },
                start: (Value(3.0), Value(0.0)),
                end: Some((Value(3.0), Value(10.0))),
            }),
        },
    ];

    for case in cases {
        assert_eq!(
            sample_pair(
                &history(case.first),
                &history(case.second),
                NetTick(case.interpolation_tick),
            ),
            case.expected,
            "{}",
            case.name,
        );
    }
}
