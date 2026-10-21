use campfire_sim::IdAllocator;

use super::*;

/// The unit of stable id `index`, as an allocator gives them from 0.
fn unit(index: u64) -> StableId {
    let mut ids = IdAllocator::default();
    let id = (0..=index).map(|_| ids.allocate()).last().unwrap();
    assert_eq!(id.get(), index);
    id
}

/// Stages above 1 and above 5 shots, of the modifiers 7 and 9, as the Gattling Cannon's, with a
/// coast of 60 ticks.
fn gattling() -> Streak {
    Streak::new(
        Ticks::new(60),
        &[(1, ModifierId::new(7)), (5, ModifierId::new(9))],
    )
}

/// The weapon 0's shot at `target` in tick `now`, ready again `period` ticks later.
fn shot(target: u64, now: u64, period: u64) -> StreakShot {
    StreakShot {
        weapon: ActionId::new(0),
        target: unit(target),
        now: Tick::new(now),
        ready: Tick::new(now + period),
    }
}

/// The count and the stage of `streak`.
fn state(streak: ShotStreak) -> (u32, Option<u8>) {
    let shots = streak.0.unwrap();
    (shots.count, shots.stage)
}

#[test]
fn a_streak_rises_a_stage_at_a_time_past_each_stages_shots() {
    // At one target, 8 ticks apart then 4: the second shot passes 1 and the sixth passes 5. Each
    // shot that leaves a stage gives the first tick past its coast, its ready tick and 60 more.
    let gattling = gattling();
    let mut streak = ShotStreak::default();
    let mut seen = Vec::new();
    for (now, period) in [(0, 8), (8, 8), (16, 4), (20, 4), (24, 4), (28, 4), (32, 2)] {
        let due = streak.shoot(shot(3, now, period), &gattling);
        seen.push((state(streak), due.map(Tick::get)));
    }
    assert_eq!(
        seen,
        [
            ((1, None), None),
            ((2, Some(0)), Some(8 + 8 + 60 + 1)),
            ((3, Some(0)), Some(16 + 4 + 61)),
            ((4, Some(0)), Some(20 + 4 + 61)),
            ((5, Some(0)), Some(24 + 4 + 61)),
            ((6, Some(1)), Some(28 + 4 + 61)),
            ((7, Some(1)), Some(32 + 2 + 61)),
        ]
    );
}

#[test]
fn a_new_target_keeps_the_count_before_the_coast_ends_and_restarts_it_after() {
    // Two shots at unit 3 hold stage 0, the coast ending in tick 8 + 8 + 60 = 76. A shot at unit 4
    // in tick 75 counts a third; one at unit 5 in tick 76, as the coast ended, starts at one,
    // which is not below stage 0's 1 shot, so the stage holds, as Zero Hour's `MEAN` does.
    let gattling = gattling();
    let mut streak = ShotStreak::default();
    streak.shoot(shot(3, 0, 8), &gattling);
    streak.shoot(shot(3, 8, 8), &gattling);
    let mut kept = streak;
    kept.shoot(shot(4, 75, 8), &gattling);
    assert_eq!(state(kept), (3, Some(0)));
    let mut restarted = streak;
    restarted.shoot(shot(5, 76, 8), &gattling);
    assert_eq!(state(restarted), (1, Some(0)));
    // At stage 1, a count of 1 is below its 5 shots, so the stage falls to none.
    let mut fast = ShotStreak::default();
    for now in 0..6 {
        fast.shoot(shot(3, now * 4, 4), &gattling);
    }
    assert_eq!(state(fast), (6, Some(1)));
    let due = fast.shoot(shot(4, 500, 4), &gattling);
    assert_eq!((state(fast), due), ((1, None), None));
}

#[test]
fn a_coast_ends_the_stage_and_keeps_the_count() {
    // Six shots at unit 3 reach stage 1; past the coast the stage ends and the count stays 6, so
    // the next shot at unit 3 rises to stage 0 at 7 and the one after to stage 1 at 8.
    let gattling = gattling();
    let mut streak = ShotStreak::default();
    for now in 0..6 {
        streak.shoot(shot(3, now * 4, 4), &gattling);
    }
    streak.0.as_mut().unwrap().stage = None;
    assert_eq!(streak.due(), None);
    streak.shoot(shot(3, 900, 8), &gattling);
    assert_eq!(state(streak), (7, Some(0)));
    streak.shoot(shot(3, 908, 8), &gattling);
    assert_eq!(state(streak), (8, Some(1)));
}

#[test]
fn another_weapons_stage_counts_as_its_last_at_most() {
    // Stage 1 of the Gattling Cannon's weapon, then a shot of weapon 1, of one stage above 2
    // shots: its stage is its last, 0, which the count of 7 keeps.
    let gattling = gattling();
    let single = Streak::new(Ticks::new(30), &[(2, ModifierId::new(4))]);
    let mut streak = ShotStreak::default();
    for now in 0..6 {
        streak.shoot(shot(3, now * 4, 4), &gattling);
    }
    let other = StreakShot {
        weapon: ActionId::new(1),
        ..shot(3, 24, 4)
    };
    streak.shoot(other, &single);
    assert_eq!(state(streak), (7, Some(0)));
    assert_eq!(streak.0.unwrap().weapon, ActionId::new(1));
}
