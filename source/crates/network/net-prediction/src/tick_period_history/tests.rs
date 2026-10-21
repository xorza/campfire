use super::*;
use campfire_net_core::Ticked;

const MS_10: Duration = Duration::from_millis(10);
const MS_20: Duration = Duration::from_millis(20);

/// Ticks 1 to 5 run at 10 ms and ticks 6 to 8 at 20 ms.
fn history_and_time() -> (TickPeriodHistory, Time<Fixed>) {
    let mut history = TickPeriodHistory::default();
    let mut time = Time::<Fixed>::from_duration(MS_10);
    for tick in 1..=8 {
        let period = if tick <= 5 { MS_10 } else { MS_20 };
        time.set_timestep(period);
        time.advance_by(period);
        history.record(NetTick(tick), time.delta());
    }
    (history, time)
}

/// Only the changes hold an entry, and a replayed tick that records its own period again
/// changes nothing.
#[test]
fn record_keeps_the_changes() {
    let (mut history, _) = history_and_time();
    history.record(NetTick(3), MS_10);

    assert!(history.periods.iter().eq([
        Ticked::new(NetTick(1), &MS_10),
        Ticked::new(NetTick(6), &MS_20)
    ]));
    assert_eq!(history.period(NetTick(0)), None);
    assert_eq!(history.period(NetTick(5)), Some(MS_10));
    assert_eq!(history.period(NetTick(6)), Some(MS_20));
}

/// The time at the end of tick 8 is 5 · 10 + 3 · 20 = 110 ms. A rewind to the end of tick 4
/// less ticks 5 to 8, 10 + 3 · 20 = 70 ms, is 40 ms. One timestep for the window, 20 ms,
/// would give 30 ms. The timestep stays that of `now`.
#[test]
fn rewind_takes_each_tick_at_its_own_period() {
    let (history, now) = history_and_time();
    assert_eq!(now.elapsed(), Duration::from_millis(110));

    let rewound = history.rewind(&now, NetTick(4), NetTick(8));

    assert_eq!(rewound.elapsed(), Duration::from_millis(40));
    assert_eq!(rewound.timestep(), MS_20);
    assert_eq!(
        history.rewind(&now, NetTick(8), NetTick(8)).elapsed(),
        Duration::from_millis(110)
    );
}

/// The entry at or before the oldest tick stays, as ticks from it to the next entry read it.
/// A shift moves each entry, and one that reaches tick 0 keeps it.
#[test]
fn forget_and_shift_keep_what_the_window_reads() {
    let (mut history, _) = history_and_time();
    history.forget_before(NetTick(5));
    assert!(history.periods.iter().eq([
        Ticked::new(NetTick(1), &MS_10),
        Ticked::new(NetTick(6), &MS_20)
    ]));
    history.forget_before(NetTick(6));
    assert!(history.periods.iter().eq([Ticked::new(NetTick(6), &MS_20)]));

    let (mut history, _) = history_and_time();
    history.shift(3);
    assert!(history.periods.iter().eq([
        Ticked::new(NetTick(4), &MS_10),
        Ticked::new(NetTick(9), &MS_20)
    ]));
    history.shift(-8);
    assert!(history.periods.iter().eq([
        Ticked::new(NetTick(0), &MS_10),
        Ticked::new(NetTick(1), &MS_20)
    ]));
}
