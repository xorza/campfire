use campfire_common::Tick;
use campfire_values::NameId;

use super::*;

#[test]
fn a_restore_check_needs_the_timers_in_order_and_numbered_below_the_next() {
    // The test mode declares 7 timers, the first `a`, and lists one script. Two of `a` set in
    // tick 0, due in ticks 2 and 1: kept by due time, then by number.
    let world = mode_match(SCRIPT, ScriptLimits::ROOMY);
    let t = TimerId::at(0);
    let mut timers = Timers::default();
    for ticks in [2, 1] {
        let start = TimerStart {
            owner: ListPlace::FIRST,
            name: t,
            after: Ticks::new(ticks),
            every: None,
            data: None,
        };
        timers.set(Tick::new(0), start);
    }
    assert!(timers.check(&world));
    // A timer of a name the mode does not declare fails, and one of a script past its list.
    let mut stranger = timers.clone();
    stranger.timers_mut()[0].name = TimerId::at(7);
    assert!(!stranger.check(&world));
    let mut unlisted = timers.clone();
    unlisted.timers_mut()[0].owner = ListPlace::new(1).unwrap();
    assert!(!unlisted.check(&world));
    let mut swapped = timers.clone();
    swapped.timers_mut().swap(0, 1);
    assert!(!swapped.check(&world));
    let mut behind = timers;
    *behind.next_seq_mut() = 1;
    assert!(!behind.check(&world));

    // A repeating timer of the longest period, due at the limit, with the count of timers at
    // the limit: it fires, and sets itself 2⁶² ticks on, within a `u64`. A period or a count
    // past the limit, or a period of no ticks, fails: a time to decode, the rest the check.
    let mut last = Timers::default();
    let start = TimerStart {
        owner: ListPlace::FIRST,
        name: t,
        after: Ticks::LIMIT,
        every: Some(Ticks::LIMIT),
        data: None,
    };
    last.set(Tick::ZERO, start);
    *last.next_seq_mut() = Tick::LIMIT.get();
    assert!(last.check(&world));
    let mut fired = last.clone();
    fired.fire();
    assert_eq!(fired.timers_mut()[0].due, Tick::new(1 << 63));
    let changed = |change: fn(&mut Timers)| {
        let mut timers = last.clone();
        change(&mut timers);
        Match::decodes(&timers) && timers.check(&world)
    };
    assert!(!changed(|timers| *timers.next_seq_mut() += 1));
    assert!(!changed(
        |timers| timers.timers_mut()[0].due = Tick::new(Tick::LIMIT.get() + 1)
    ));
    assert!(!changed(
        |timers| timers.timers_mut()[0].every = Some(Ticks::new(Tick::LIMIT.get() + 1))
    ));
    assert!(!changed(
        |timers| timers.timers_mut()[0].every = Some(Ticks::ZERO)
    ));
}
