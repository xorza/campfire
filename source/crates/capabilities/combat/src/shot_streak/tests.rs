use campfire_declarations::ByIdBuilder;
use campfire_math::Num;
use campfire_sim::IdAllocator;
use campfire_values::DamageKind;

use super::*;
use crate::weapon::{Rate, WeaponDamage};

/// The unit of stable id `index`, as an allocator gives them from 0.
fn unit(index: u64) -> StableId {
    let mut ids = IdAllocator::default();
    let id = (0..=index).map(|_| ids.allocate()).last().unwrap();
    assert_eq!(id.get(), index);
    id
}

/// Stages above 1 and above 5 shots, of the modifiers 7 and 9, as the Gattling Cannon's, with a
/// coast of 60 ticks, and a fall of the modifier 11 held 30 ticks.
fn gattling() -> Streak {
    Streak::new(
        Ticks::new(60),
        &[(1, ModifierId::new(7)), (5, ModifierId::new(9))],
    )
    .with_fall(ModifierId::new(11), Ticks::new(30))
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
fn state(streak: ShotStreak) -> (u32, Heat) {
    let shots = streak.0.unwrap();
    (shots.count, shots.stage)
}

/// A streak of six shots at unit 3, 4 ticks apart from tick 0, which reaches stage 1 and coasts
/// until tick 20 + 4 + 60 = 84.
fn fast(streak: &Streak) -> ShotStreak {
    let mut fast = ShotStreak::default();
    for now in 0..6 {
        fast.shoot(shot(3, now * 4, 4), streak);
    }
    assert_eq!(state(fast), (6, Heat::Stage(1)));
    fast
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
            ((1, Heat::Cold), None),
            ((2, Heat::Stage(0)), Some(8 + 8 + 60 + 1)),
            ((3, Heat::Stage(0)), Some(16 + 4 + 61)),
            ((4, Heat::Stage(0)), Some(20 + 4 + 61)),
            ((5, Heat::Stage(0)), Some(24 + 4 + 61)),
            ((6, Heat::Stage(1)), Some(28 + 4 + 61)),
            ((7, Heat::Stage(1)), Some(32 + 2 + 61)),
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
    assert_eq!(state(kept), (3, Heat::Stage(0)));
    let mut restarted = streak;
    restarted.shoot(shot(5, 76, 8), &gattling);
    assert_eq!(state(restarted), (1, Heat::Stage(0)));
    // Below every stage, past its coast, ending in tick 0 + 8 + 60 = 68: a shot at the same unit
    // in tick 69 counts one, as the streak cooled down in it, and one in tick 68 counts two.
    let mut cold = ShotStreak::default();
    cold.shoot(shot(3, 0, 8), &gattling);
    let mut late = cold;
    late.shoot(shot(3, 69, 8), &gattling);
    assert_eq!(state(late), (1, Heat::Cold));
    cold.shoot(shot(3, 68, 8), &gattling);
    assert_eq!(state(cold), (2, Heat::Stage(0)));
}

#[test]
fn a_shot_below_its_stage_cools_down_to_the_fall_and_the_next_counts_one() {
    // At stage 1, a shot of weapon 1, of stages above 10 and 20 shots, counts 7, below the 20 of
    // its stage 1: the streak falls, its count and target gone, held to the shot's coast's end.
    let gattling = gattling();
    let wide = Streak::new(
        Ticks::new(30),
        &[(10, ModifierId::new(4)), (20, ModifierId::new(5))],
    )
    .with_fall(ModifierId::new(6), Ticks::new(30));
    let mut streak = fast(&gattling);
    let other = StreakShot {
        weapon: ActionId::new(1),
        ..shot(3, 24, 4)
    };
    let due = streak.shoot(other, &wide);
    assert_eq!(state(streak), (0, Heat::Fallen));
    assert_eq!(streak.0.unwrap().target, None);
    assert_eq!(due, Some(Tick::new(24 + 4 + 30 + 1)));
    // The next shot, of the Gattling's weapon at the same unit, counts one, and the fall holds; the
    // one after, two, past stage 0's 1 shot, rises to it.
    streak.shoot(shot(3, 28, 4), &gattling);
    assert_eq!(state(streak), (1, Heat::Fallen));
    streak.shoot(shot(3, 32, 4), &gattling);
    assert_eq!(state(streak), (2, Heat::Stage(0)));
    // A weapon of no fall goes cold, and a shot of it while fallen leaves the fall too, as it
    // holds none.
    let bare = Streak::new(
        Ticks::new(30),
        &[(10, ModifierId::new(4)), (20, ModifierId::new(5))],
    );
    let mut streak = fast(&gattling);
    let due = streak.shoot(other, &bare);
    assert_eq!((state(streak), due), ((0, Heat::Cold), None));
    let mut fallen = fast(&gattling);
    fallen.cool(Tick::new(85), Some(&gattling));
    let due = fallen.shoot(
        StreakShot {
            weapon: ActionId::new(1),
            ..shot(3, 90, 4)
        },
        &bare,
    );
    assert_eq!((state(fallen), due), ((1, Heat::Cold), None));
}

#[test]
fn a_coast_cools_down_to_the_fall_and_its_end_stops_the_streak() {
    // Stage 1, its coast ending in tick 84: in tick 85 it falls, held 30 ticks, to tick 115, so
    // due in 116; then it stops, and a shot at unit 3 counts one.
    let gattling = gattling();
    let mut streak = fast(&gattling);
    assert_eq!(streak.due(), Some(Tick::new(85)));
    let due = streak.cool(Tick::new(85), Some(&gattling));
    assert_eq!(
        (state(streak), due),
        ((0, Heat::Fallen), Some(Tick::new(116)))
    );
    let mut fallen = streak;
    assert_eq!(streak.cool(Tick::new(116), Some(&gattling)), None);
    assert_eq!(streak, ShotStreak::default());
    streak.shoot(shot(3, 200, 4), &gattling);
    assert_eq!(state(streak), (1, Heat::Cold));
    // A shot during the fall counts one and keeps it; one of no stage past the fall's coast
    // stops too, a weapon of no fall stops at once, and one of no stage cools to nothing.
    fallen.shoot(shot(3, 100, 4), &gattling);
    assert_eq!(state(fallen), (1, Heat::Fallen));
    let bare = Streak::new(Ticks::new(60), &[(1, ModifierId::new(7))]);
    let mut stopped = fast(&gattling);
    assert_eq!(stopped.cool(Tick::new(85), Some(&bare)), None);
    assert_eq!(stopped, ShotStreak::default());
}

#[test]
fn the_modifier_is_the_stages_or_the_falls() {
    // The book's weapon 0 of the Gattling's streak: stage 1's 9, the fall's 11; none once
    // stopped.
    let gattling = gattling();
    let weapon = Weapon {
        rate: Rate::Period(Box::new([Ticks::new(4)])),
        rate_bonus: None,
        range_bonus: None,
        damage: WeaponDamage::Amount(Box::new([Num::ONE])),
        damage_bonus: None,
        kind: DamageKind::new(0),
        prefers: None,
        clip: None,
        splash: None,
        scatter: None,
        pitch: None,
        speed: None,
        streak: Some(gattling.clone()),
        pre_attack: None,
        aim: None,
    };
    let mut weapons = ByIdBuilder::default();
    weapons.set(ActionId::new(0), weapon);
    let weapons = weapons.build();
    let mut streak = fast(&gattling);
    assert_eq!(streak.modifier(&weapons), Some(ModifierId::new(9)));
    streak.cool(Tick::new(85), Some(&gattling));
    assert_eq!(streak.modifier(&weapons), Some(ModifierId::new(11)));
    streak.cool(Tick::new(116), Some(&gattling));
    assert_eq!(streak.modifier(&weapons), None);
}
