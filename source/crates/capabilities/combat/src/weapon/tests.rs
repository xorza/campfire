use std::str::FromStr;

use campfire_geometry::Placed;

use super::*;

/// A weapon of the first two stats' rate and damage.
fn weapon() -> Weapon {
    Weapon {
        rate: Rate::Stat(StatId::new(0)),
        damage: WeaponDamage::Stat(StatId::new(1)),
        ..own([3, 2], [5, 0])
    }
}

/// A weapon of its own periods and amounts at its two ranks, in ticks.
fn own(periods: [u64; 2], amounts: [i64; 2]) -> Weapon {
    Weapon {
        rate: Rate::Period(Box::new(periods.map(Ticks::new))),
        rate_bonus: None,
        damage: WeaponDamage::Amount(Box::new(amounts.map(Num::int))),
        damage_bonus: None,
        kind: DamageKind::new(0),
        clip: None,
        splash: None,
        scatter: None,
        pitch: None,
        speed: None,
        streak: None,
        pre_attack: None,
        aim: None,
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
        let got = weapon().period(values, Rank::FIRST, Ticks::new(windup), hz);
        assert_eq!(got, Ticks::new(period), "{values:?} at {hz} after {windup}");
    }
}

#[test]
fn damage_is_the_stat_at_its_place_and_never_negative() {
    let damage = |stats: &[Num]| weapon().damage(stats, Rank::FIRST);
    let rate = Num::ONE;
    assert_eq!(damage(&[rate, decimal("23.5")]), decimal("23.5"));
    assert_eq!(damage(&[rate, decimal("-5")]), Num::ZERO);
    assert_eq!(damage(&[rate]), Num::ZERO);
}

#[test]
fn a_weapon_of_its_own_period_and_amount_reads_them_at_its_rank_and_no_stat() {
    // Periods of 3 and 2 ticks: rank 1's 3 after no windup is 3; after a windup of 3, it must
    // outlast it: 4; rank 2's 2 is 2. Its amounts of 5 and 0 are dealt whatever the stats hold.
    let stats = [Num::ONE, decimal("23.5")];
    let weapon = own([3, 2], [5, 0]);
    let second = Rank::new(2).unwrap();
    assert_eq!(
        weapon.period(&stats, Rank::FIRST, Ticks::ZERO, 30),
        Ticks::new(3)
    );
    assert_eq!(
        weapon.period(&stats, Rank::FIRST, Ticks::new(3), 30),
        Ticks::new(4)
    );
    assert_eq!(
        weapon.period(&stats, second, Ticks::ZERO, 30),
        Ticks::new(2)
    );
    assert_eq!(weapon.damage(&stats, Rank::FIRST), Num::int(5));
    assert_eq!(weapon.damage(&[], second), Num::ZERO);
    // A clip at each rank reads its own rank's.
    let clips = [3, 5].map(|shots| ClipRule {
        shots: NonZeroU16::new(shots).unwrap(),
        reload: Ticks::new(21),
    });
    let clipped = Weapon {
        clip: Some(Box::new(clips)),
        ..weapon
    };
    assert_eq!(clipped.clip(&stats, second), Some(clips[1]));
    assert_eq!(own([1, 1], [0, 0]).clip(&stats, Rank::FIRST), None);
}

#[test]
fn a_rate_bonus_divides_its_own_period_and_reload_by_one_more_than_it_rounded_down() {
    // A period of 8 ticks and a reload of 21, by the stat 0: at 0, 8 and 21; at 1, 4 and 10.5,
    // down to 10; at 2, 8 ÷ 3 = 2.67, down to 2, and 7; at −0.5, 16 and 42. At −1 or below, a
    // divisor of one bit: 8 × 2²⁴ and 21 × 2²⁴ ticks. A period stays past a windup of 3: at 2,
    // 4 ticks in place of 2.
    let clip = ClipRule {
        shots: NonZeroU16::new(3).unwrap(),
        reload: Ticks::new(21),
    };
    let weapon = Weapon {
        rate_bonus: Some(StatId::new(0)),
        clip: Some(Box::new([clip, clip])),
        ..own([8, 8], [1, 1])
    };
    let ticks = |bonus: Num, windup: u64| {
        let stats = [bonus];
        let period = weapon.period(&stats, Rank::FIRST, Ticks::new(windup), 30);
        let reload = weapon.clip(&stats, Rank::FIRST).unwrap().reload;
        [period.get(), reload.get()]
    };
    assert_eq!(ticks(Num::ZERO, 0), [8, 21]);
    assert_eq!(ticks(Num::ONE, 0), [4, 10]);
    assert_eq!(ticks(Num::int(2), 0), [2, 7]);
    assert_eq!(ticks(-Num::HALF, 0), [16, 42]);
    assert_eq!(ticks(-Num::ONE, 0), [8 << 24, 21 << 24]);
    assert_eq!(ticks(Num::int(-3), 0), [8 << 24, 21 << 24]);
    assert_eq!(ticks(Num::int(2), 3), [4, 7]);
    // A unit that holds no value of the stat fires at the weapon's own period.
    assert_eq!(weapon.period(&[], Rank::FIRST, Ticks::ZERO, 30).get(), 8);
}

#[test]
fn a_damage_bonus_scales_its_amount_and_its_outer_ring_rounded_once() {
    // 10 and an outer ring of 4, by the stat 0: at 0.5, 15 and 6; at 0, 10 and 4; at −0.25,
    // 7.5 and 3; at −1 or below, a factor of 0: 0 and 0; at one bit, 2⁻²⁴, 10 × (1 + 2⁻²⁴) is
    // 10 + 10 × 2⁻²⁴, ten bits more. With no bonus, or a stat the unit lacks, the weapon deals
    // its own amounts.
    let splash = Splash::new(Num::ONE, RelationSet::Enemies, true)
        .and_then(|splash| splash.with_outer(Num::int(3), Box::new([Num::int(4), Num::int(4)])))
        .unwrap();
    let weapon = Weapon {
        damage_bonus: Some(StatId::new(0)),
        splash: Some(splash),
        ..own([1, 1], [10, 10])
    };
    let dealt = |weapon: &Weapon, stats: &[Num]| {
        let outer = weapon.outer_amount(stats, Rank::FIRST).unwrap();
        [weapon.damage(stats, Rank::FIRST), outer]
    };
    assert_eq!(dealt(&weapon, &[Num::HALF]), [Num::int(15), Num::int(6)]);
    assert_eq!(dealt(&weapon, &[Num::ZERO]), [Num::int(10), Num::int(4)]);
    assert_eq!(
        dealt(&weapon, &[decimal("-0.25")]),
        [decimal("7.5"), Num::int(3)]
    );
    assert_eq!(dealt(&weapon, &[-Num::ONE]), [Num::ZERO, Num::ZERO]);
    assert_eq!(dealt(&weapon, &[Num::int(-3)]), [Num::ZERO, Num::ZERO]);
    let bit = Num::from_bits(1);
    assert_eq!(
        dealt(&weapon, &[bit])[0],
        Num::from_bits(Num::int(10).to_bits() + 10)
    );
    assert_eq!(dealt(&weapon, &[]), [Num::int(10), Num::int(4)]);
    let plain = Weapon {
        damage_bonus: None,
        ..weapon
    };
    assert_eq!(dealt(&plain, &[Num::HALF]), [Num::int(10), Num::int(4)]);
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
        ..own([1, 1], [0, 0])
    };
    let delay = |distance: Num| slow.delay(distance, 30);
    let e = Num::EPSILON;
    assert_eq!(delay(Num::int(15) / 2), Some(Ticks::new(8)));
    assert_eq!(delay(Num::ONE), Some(Ticks::new(1)));
    assert_eq!(delay(Num::ONE + e), Some(Ticks::new(2)));
    assert_eq!(delay(Num::ONE - e), None);
    assert_eq!(delay(Num::ZERO), None);
    assert_eq!(own([1, 1], [0, 0]).delay(Num::int(10), 30), None);
    // At 60 Hz the same weapon flies half a meter a tick: 1 m is 2 ticks.
    assert_eq!(slow.delay(Num::ONE, 60), Some(Ticks::new(2)));
}
