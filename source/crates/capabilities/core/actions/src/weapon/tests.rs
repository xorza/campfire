use std::str::FromStr;

use campfire_stats::PoolCost;

use campfire_geometry::Placed;

use super::*;
use crate::action_range::ActionRange;

fn weapon() -> Weapon {
    Weapon {
        rate: Source::Stat(StatId::new(0)),
        damage: Source::Stat(StatId::new(1)),
        ..OWN
    }
}

/// A weapon of its own period and amount.
const OWN: Weapon = Weapon {
    rate: Source::Own,
    damage: Source::Own,
    kind: DamageKind::new(0),
    splash: None,
    scatter: None,
    pitch: None,
    speed: None,
};

/// A rank's values of `windup` ticks, its own period `period` and amount `amount`.
fn values(windup: u64, period: Option<u64>, amount: Option<Num>) -> RankValues {
    RankValues {
        range: ActionRange::Global,
        cooldown: Ticks::ZERO,
        cost: PoolCost::default(),
        windup: Ticks::new(windup),
        charges: None,
        toggle: None,
        channel: None,
        charge: None,
        amount,
        period: period.map(Ticks::new),
        clip: None,
    }
}

fn decimal(text: &str) -> Num {
    Num::from_str(text).unwrap()
}

#[test]
fn a_period_is_the_tick_rate_over_the_rate_rounded_up_and_past_the_windup() {
    // 0.67 attacks a second is 11240734.72 / 2²⁴, to 11240735: 30 × 2²⁴ over it is 44.78,
    // up to 45 ticks; 20 × 2²⁴ over it, 29.85, up to 30. Three a second is 10 ticks at 30
    // exactly, but must outlast a windup of 10. No rate, or one not positive, counts as one
    // bit: 30 × 2²⁴ ticks; at the highest tick rate, (2³² − 1) × 2²⁴, past what a `Num` holds.
    let slow = [decimal("0.67")];
    let three = [Num::from_int(3).unwrap()];
    let none = [Num::from_int(-2).unwrap()];
    let cases = [
        (&slow[..], 30, 0, 45),
        (&slow[..], 20, 0, 30),
        (&three[..], 30, 9, 10),
        (&three[..], 30, 10, 11),
        (&none[..], 30, 0, 30 << 24),
        (&[][..], 30, 0, 30 << 24),
        (&none[..], u32::MAX, 0, u64::from(u32::MAX) << 24),
    ];
    for (values, hz, windup, period) in cases {
        let got = weapon().period(values, &self::values(windup, None, None), hz);
        assert_eq!(got, Ticks::new(period), "{values:?} at {hz} after {windup}");
    }
}

#[test]
fn damage_is_the_stat_at_its_place_and_never_negative() {
    let own = values(0, None, None);
    let damage = |stats: &[Num]| weapon().damage(stats, &own);
    let rate = Num::ONE;
    assert_eq!(damage(&[rate, decimal("23.5")]), decimal("23.5"));
    assert_eq!(damage(&[rate, decimal("-5")]), Num::ZERO);
    assert_eq!(damage(&[rate]), Num::ZERO);
}

#[test]
fn a_weapon_of_its_own_period_and_amount_reads_them_at_its_rank_and_no_stat() {
    // A period of 3 ticks after no windup is 3; after a windup of 3, it must outlast it: 4. Its
    // amount of 5 is dealt whatever its unit's stats hold.
    let stats = [Num::ONE, decimal("23.5")];
    assert_eq!(
        OWN.period(&stats, &values(0, Some(3), None), 30),
        Ticks::new(3)
    );
    assert_eq!(
        OWN.period(&stats, &values(3, Some(3), None), 30),
        Ticks::new(4)
    );
    let five = Num::from_int(5).unwrap();
    assert_eq!(OWN.damage(&stats, &values(0, None, Some(five))), five);
    assert_eq!(
        OWN.damage(&[], &values(0, None, Some(Num::ZERO))),
        Num::ZERO
    );
}

#[test]
fn a_scatter_widens_at_its_targets_tags_and_aims_by_its_two_draws() {
    // 2 m, and 3 m more at the tag 0 and 1 m more at the tag 1: a target of both takes the
    // larger, 5 m; of none, 2 m.
    let tag = |index: u8, radius: i64| ScatterTag {
        tag: Tag::new(index),
        radius: Num::int(radius),
    };
    let scatter = Scatter {
        radius: Num::int(2),
        tags: Box::new([tag(0, 3), tag(1, 1)]),
    };
    let both = TagSet::default().with(Tag::new(0)).with(Tag::new(1));
    assert_eq!(scatter.radius(both), Num::int(5));
    assert_eq!(
        scatter.radius(TagSet::default().with(Tag::new(1))),
        Num::int(3)
    );
    assert_eq!(scatter.radius(TagSet::default()), Num::int(2));
    // From (10, 0, 4), its draws ½ and ¼, a quarter turn from x toward −z: 1 m, to (10, 0, 3);
    // and ¾ and ½, half a turn: 1.5 m, to (8.5, 0, 4).
    let at = Vec3::from((10, 7, 4));
    let half = Num::HALF;
    let drawn = |draws| scatter.point(at, TagSet::default(), || draws);
    assert_eq!(drawn([half, Num::QUARTER]), Some(Vec3::from((10, 0, 3))));
    let three = Num::QUARTER * 3;
    assert_eq!(
        drawn([three, half]),
        Some(Vec3::new(Num::int(8) + half, Num::ZERO, Num::int(4)))
    );
    // A radius of 0 aims at the target and draws nothing.
    let none = Scatter {
        radius: Num::ZERO,
        tags: Box::new([]),
    };
    assert_eq!(
        none.point(at, TagSet::default(), || panic!("no draw")),
        None
    );
}

#[test]
fn a_pitch_bound_reaches_a_target_whose_pitches_from_bottom_to_top_meet_it() {
    // From −10° to 45°, from (1, 2, 3): a point 3 m up and 4 m along, 36.9°, is within; 4 m up
    // and 3 m along, 53.1°, past it; 45° exactly is within; 1 m down and 4 m along, −14°, below
    // it; level is within.
    let pitch = Pitch::of(Num::int(-10), Num::int(45));
    let from = Vec3::from((1, 2, 3));
    let volume = |along: i32, up: i32, height: i64| {
        let base = Placed::point(from + Vec3::from((along, up, 0)));
        Volume::new(base, Num::int(height))
    };
    let reached = [(4, 3), (3, 4), (5, 5), (4, -1), (4, 0)]
        .map(|(along, up)| pitch.reaches(from, &volume(along, up, 0)));
    assert_eq!(reached, [true, false, true, false, true]);
    // A volume reaches when any pitch of it, bottom to top, does: one from 1 m down to 3 m up,
    // 4 m along, spans −14° to 36.9°; one from 4 m up to 10 m up, 3 m along, spans 53.1° and
    // more, past it.
    assert!(pitch.reaches(from, &volume(4, -1, 4)));
    assert!(!pitch.reaches(from, &volume(3, 4, 6)));
    // From −90° to 90°, any.
    let any = Pitch::of(Num::int(-90), Num::int(90));
    let all = [(0, -5), (0, 5), (5, 0)].map(|(along, up)| any.reaches(from, &volume(along, up, 0)));
    assert_eq!(all, [true; 3]);
}

#[test]
fn a_slow_weapons_hit_lands_its_distance_over_its_speed_a_tick_after_its_shot() {
    // 30 m/s at 30 Hz, 1 m a tick: 7.5 m lands in ⌈7.5⌉ = 8 ticks, 1 m in exactly 1, a bit
    // more in 2, a bit less at once, 0 m at once; a weapon of no speed at once.
    let slow = Weapon {
        speed: Speed::new(Num::int(30)),
        ..OWN
    };
    let delay = |distance: Num| slow.delay(distance, 30);
    let e = Num::EPSILON;
    assert_eq!(delay(Num::int(15) / 2), Some(Ticks::new(8)));
    assert_eq!(delay(Num::ONE), Some(Ticks::new(1)));
    assert_eq!(delay(Num::ONE + e), Some(Ticks::new(2)));
    assert_eq!(delay(Num::ONE - e), None);
    assert_eq!(delay(Num::ZERO), None);
    assert_eq!(OWN.delay(Num::int(10), 30), None);
    // At 60 Hz the same weapon flies half a meter a tick: 1 m is 2 ticks.
    assert_eq!(slow.delay(Num::ONE, 60), Some(Ticks::new(2)));
}
