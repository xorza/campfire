use std::cell::Cell;
use std::num::NonZeroU32;

use super::*;

/// The halves and quarters of a meter, exact in a `Num`.
fn num(value: f64) -> Num {
    #[expect(
        clippy::float_arithmetic,
        reason = "a test writes its exact dyadic numbers in decimal"
    )]
    let bits = value * f64::from(1 << Num::FRAC_BITS);
    assert_eq!(bits.fract(), 0.0, "{value} is no dyadic number");
    Num::from_bits(bits as i64)
}

fn point(x: f64, z: f64) -> Vec3 {
    Vec3::new(num(x), Num::ZERO, num(z))
}

fn facing(degrees: f64) -> Facing {
    Facing::of(num(degrees))
}

/// A locomotor of `appearance` at 2 ticks a second, so each number a tick is dyadic: it brakes
/// 6 m/s², 1.5 m a tick each tick, and loses a quarter of its lateral velocity a tick; a wheel
/// waits 5 ticks near its goal and looks a tick ahead in a turn.
fn mover(
    appearance: LocomotorAppearance,
    edit: impl FnOnce(&mut ZeroHourLocomotor),
) -> ZeroHourMover {
    let mut locomotor = ZeroHourLocomotor {
        lateral_friction: Num::QUARTER,
        ..ZeroHourLocomotor::of(appearance, Num::int(6))
    };
    edit(&mut locomotor);
    ZeroHourMover::of(&locomotor, MatchRate::new(NonZeroU32::new(2).unwrap()))
}

/// Steers at 3 m a tick, turning `turn` degrees a tick and gaining 1.5 m a tick each tick,
/// toward `goal`, `on_path` meters from the route's end.
fn steer(goal: Vec3, on_path: f64, turn: f64) -> Steer {
    Steer {
        speed: Num::int(3),
        turn: num(turn),
        accel: num(1.5),
        goal,
        on_path: num(on_path),
    }
}

fn moving(velocity: [f64; 2]) -> ZeroHourDrive {
    ZeroHourDrive {
        velocity: velocity.map(num),
        ..ZeroHourDrive::default()
    }
}

/// Drives `ticks` ticks from `stance` and `drive` toward `goal` along the x axis, its route's end
/// `goal` meters on, on open ground; each tick's point, facing and velocity.
fn drive_along(
    mover: &ZeroHourMover,
    stance: &mut Stance,
    drive: &mut ZeroHourDrive,
    ticks: usize,
    turn: f64,
    goal: Vec3,
) -> Vec<(Vec3, Facing, [Num; 2])> {
    (0..ticks)
        .map(|_| {
            let on_path = stance.at.ground().distance(goal.ground());
            let steer = Steer {
                on_path,
                ..steer(goal, 0.0, turn)
            };
            mover.drive(steer, stance, drive, |_| true);
            (stance.at, stance.facing, drive.velocity)
        })
        .collect()
}

#[test]
fn a_mover_takes_its_numbers_a_tick_at_the_match_rate() {
    // At Zero Hour's 30 frames a second a Humvee's braking of 1,000 m/s² is 1000 / 900 m a tick
    // each tick, its turning speed of 20 m/s 2 / 3 m a tick, each to the nearest bit; it lands at
    // a cell a second, 1 / 3 m a tick, stops an axis below 0.001 m a tick, looks 15 ticks ahead
    // and waits 75 near its goal.
    let humvee = ZeroHourLocomotor {
        min_turn_speed: Num::int(20),
        ..ZeroHourLocomotor::of(LocomotorAppearance::FourWheels, Num::int(1000))
    };
    let mover = ZeroHourMover::of(&humvee, MatchRate::new(NonZeroU32::new(30).unwrap()));
    let nearest = |numerator: i64, denominator: i64| {
        Num::ONE
            .checked_mul_div_int(numerator, denominator, Rounding::NearestEven)
            .unwrap()
    };
    assert_eq!(mover.braking, nearest(1000, 900));
    assert_eq!(mover.min_turn_speed, nearest(2, 3));
    assert_eq!(mover.landing, nearest(1, 3));
    assert_eq!(mover.still, "0.001".parse().unwrap());
    assert_eq!((mover.projected, mover.donut), (15, 75));
    assert_eq!(mover.donut_past(), 76);
    // The constants are the game's to the nearest bit.
    for (constant, decimal) in [
        (TWITCH, "0.1"),
        (TINY, "0.001"),
        (NEAR_TURN, "0.05"),
        (START_FACTOR, "1.1"),
    ] {
        assert_eq!(constant, decimal.parse().unwrap(), "{decimal}");
    }
}

#[test]
fn the_forward_speed_is_the_release_games_on_each_axis_apart() {
    // Along its facing on an axis, the speed is the velocity's; against it, negative. On a
    // diagonal, each axis's part is v·r² for r = cos 45°, so the speed is √2·v·r² ≈ 0.707·v, not
    // v (driving.md, The release game's forward speed).
    assert_eq!(
        ZeroHourMover::forward([Num::int(2), Num::ZERO], facing(0.0)),
        Num::int(2)
    );
    assert_eq!(
        ZeroHourMover::forward([Num::ZERO, Num::int(2)], facing(90.0)),
        -Num::int(2)
    );
    let r = SinCos::of_degrees(Num::int(45)).cos;
    let part = Num::int(2) * r * r;
    let diagonal = ZeroHourMover::forward([Num::int(2) * r, -(Num::int(2) * r)], facing(45.0));
    assert_eq!(diagonal, (part * part * 2).sqrt());
    assert!(diagonal < Num::int(2) && diagonal > num(1.375));
}

#[test]
fn a_tread_gathers_speed_on_an_axis_and_brakes_and_lands_on_its_goal() {
    let tread = mover(LocomotorAppearance::Treads, |_| {});
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive::default();
    // From a stop, 1.5 m a tick each tick up to its 3: 1.5, then 3 and 3.
    let ticks = drive_along(&tread, &mut stance, &mut drive, 3, 90.0, point(100.0, 0.0));
    let points: Vec<_> = ticks.iter().map(|&(at, _, [x, z])| (at.x, x, z)).collect();
    assert_eq!(
        points,
        [(1.5, 1.5), (4.5, 3.0), (7.5, 3.0)].map(|(at, x)| (num(at), num(x), Num::ZERO))
    );
    // At 3 m a tick, it slows down within (3 / 1.5) × (3 / 1.5) = 4 m: from 9 m, 1 m short of
    // its goal at 10, it brakes at the factor (4 / 1)², at most 5, to 3 − 1.5 = 1.5, and stands
    // as it brakes. Then its slow-down is 1 × 1 = 1 m, within a quarter of the meter left, so it
    // brakes half its braking, to 0.75, at the factor (1 / 1)², and lands the meter left, as it
    // braked as its last tick ended.
    let mut stance = Stance {
        at: point(9.0, 0.0),
        facing: facing(0.0),
    };
    let mut drive = moving([3.0, 0.0]);
    let ticks = drive_along(&tread, &mut stance, &mut drive, 2, 90.0, point(10.0, 0.0));
    assert_eq!(
        ticks,
        [
            (point(9.0, 0.0), facing(0.0), [num(1.5), Num::ZERO]),
            (point(10.0, 0.0), facing(0.0), [num(0.75), Num::ZERO]),
        ]
    );
    assert!(drive.braking && drive.braked);
    assert_eq!(drive.factor, Num::ONE);
    // From 3 m short, its 4 m of slow-down start no braking: it starts only once the meters left
    // fall below them.
    let mut stance = Stance {
        at: point(7.0, 0.0),
        facing: facing(0.0),
    };
    let mut drive = moving([3.0, 0.0]);
    drive_along(&tread, &mut stance, &mut drive, 1, 90.0, point(11.0, 0.0));
    assert_eq!((stance.at, drive.braking), (point(10.0, 0.0), false));
}

#[test]
fn a_tread_on_a_diagonal_drives_faster_than_its_speed() {
    // Toward 45°, the push of 1.5 m lies along the diagonal, with no lateral part to lose; as its
    // forward speed is √2·|v|·r², it gathers velocity until that is its 3 m a tick, |v| = 3√2.
    let tread = mover(LocomotorAppearance::Treads, |_| {});
    let r = SinCos::of_degrees(Num::int(45)).cos;
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(45.0),
    };
    let mut drive = ZeroHourDrive::default();
    let goal = point(1000.0, -1000.0);
    let ticks = drive_along(&tread, &mut stance, &mut drive, 40, 90.0, goal);
    assert_eq!(ticks[0].2, [num(1.5) * r, -(num(1.5) * r)]);
    let [x, z] = drive.velocity;
    assert_eq!(x, -z);
    // Each tick adds its rounding to the velocity's, so its length lies within a few bits of the
    // speed's √2 times, which the axis's 3 is not.
    let length = (x * x + z * z).sqrt();
    let expected = Num::int(18).sqrt();
    assert!(
        (length - expected).to_bits().abs() < 16,
        "{length:?} against {expected:?}"
    );
}

#[test]
fn a_tread_stands_to_turn_past_45_degrees_and_slows_below() {
    // Its goal 90° round, at 22.5° a tick: past 45°, and at 45° itself, its goal speed is
    // (1 − 1) × 3 = 0, and it stands as it turns; at 22.5° short it aims at (1 − 0.5) × 3 = 1.5
    // and gains it in one push, along its new facing, north.
    let tread = mover(LocomotorAppearance::Treads, |_| {});
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive::default();
    let goal = point(0.0, -100.0);
    let ticks = drive_along(&tread, &mut stance, &mut drive, 4, 22.5, goal);
    assert_eq!(
        ticks,
        [
            (Vec3::ZERO, facing(22.5), [Num::ZERO; 2]),
            (Vec3::ZERO, facing(45.0), [Num::ZERO; 2]),
            (Vec3::ZERO, facing(67.5), [Num::ZERO; 2]),
            (point(0.0, -1.5), facing(90.0), [Num::ZERO, num(-1.5)]),
        ]
    );
}

#[test]
fn a_turn_swings_the_body_round_its_pivot_but_not_while_it_brakes() {
    // A pivot 2 m behind its center, at (−2, 0); the goal due north of the pivot: it turns 90°
    // about the pivot, so its center swings to (−2, −2), and its goal speed is 0.
    let tread = mover(LocomotorAppearance::Treads, |locomotor| {
        locomotor.pivot = Num::int(-2);
    });
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive::default();
    tread.drive(
        steer(point(-2.0, -100.0), 100.0, 90.0),
        &mut stance,
        &mut drive,
        |_| true,
    );
    assert_eq!(
        (stance.at, stance.facing),
        (point(-2.0, -2.0), facing(90.0))
    );
    // Braking, 5 m from its route's end, it turns about its center toward its goal due north of
    // it, and stays where it stands.
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive {
        braking: true,
        ..ZeroHourDrive::default()
    };
    tread.drive(
        steer(point(0.0, -5.0), 5.0, 90.0),
        &mut stance,
        &mut drive,
        |_| true,
    );
    assert_eq!((stance.at, stance.facing), (Vec3::ZERO, facing(90.0)));
    // A goal within 0.1 m of its pivot does not turn it.
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive::default();
    tread.drive(
        steer(point(-2.0, -0.0625), 100.0, 90.0),
        &mut stance,
        &mut drive,
        |_| true,
    );
    assert_eq!(stance.facing, facing(0.0));
}

#[test]
fn a_leg_slows_to_its_least_speed_within_its_slow_down() {
    // At 3 m a tick it slows down to its least speed of 0 within 3² / 1.5 / 2 × 1.05 = 3.15 m:
    // 3 m from its route's end it aims at 0 and brakes a tick's 1.5; 4 m from it, it keeps its
    // 3. A leg has no braking flag, so it moves as it slows.
    let leg = mover(LocomotorAppearance::TwoLegs, |_| {});
    let drive_from = |on_path: f64| {
        let mut stance = Stance {
            at: Vec3::ZERO,
            facing: facing(0.0),
        };
        let mut drive = moving([3.0, 0.0]);
        leg.drive(
            steer(point(100.0, 0.0), on_path, 90.0),
            &mut stance,
            &mut drive,
            |_| true,
        );
        (stance.at.x, drive.velocity[0], drive.braking)
    };
    assert_eq!(drive_from(3.0), (num(1.5), num(1.5), false));
    assert_eq!(drive_from(4.0), (num(3.0), num(3.0), false));
    // It turns toward the way to its goal, measuring its speed before it turns: from a stop it
    // stands past 45° as a tread does.
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive::default();
    let ticks = drive_along(&leg, &mut stance, &mut drive, 3, 45.0, point(0.0, -100.0));
    assert_eq!(
        ticks,
        [
            (Vec3::ZERO, facing(45.0), [Num::ZERO; 2]),
            (Vec3::ZERO, facing(90.0), [Num::ZERO; 2]),
            (point(0.0, -1.5), facing(90.0), [Num::ZERO, num(-1.5)]),
        ]
    );
}

#[test]
fn a_wheel_holds_its_turning_speed_and_slides_in_a_turn() {
    // At 1.5 m a tick east, its goal due north: past 9° it aims at its turning speed, 3 / 4 =
    // 0.75; its arc ahead is open, and at more than its turning speed it turns the whole 90°,
    // then pushes −0.75 along its new facing, as it measured its speed before it turned. Its
    // velocity east is lateral now, and loses a quarter of it: 1.5 − 0.375 = 1.125.
    let wheel = mover(LocomotorAppearance::FourWheels, |_| {});
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = moving([1.5, 0.0]);
    wheel.drive(
        steer(point(0.0, -100.0), 100.0, 90.0),
        &mut stance,
        &mut drive,
        |_| true,
    );
    assert_eq!(stance.facing, facing(90.0));
    assert_eq!(drive.velocity, [num(1.125), num(0.75)]);
    assert_eq!(stance.at, point(1.125, 0.75));
}

#[test]
fn a_wheel_turns_where_it_stands_before_an_arc_into_a_wall() {
    // As above, its arc half a second ahead, a tick at 2 a second, swings 22.5° by
    // (0.75 + 1.5) / 2 = 1.125 m: its middle, about 0.52 m east, lies before a wall from 1 m
    // east, and its end, about 1.04 m east, past it. So it turns its whole turn step where it
    // stands, with no push; only the lateral friction slows it.
    let wheel = mover(LocomotorAppearance::FourWheels, |_| {});
    // The points it tests past the one it stands on, which it tests first, in the open.
    let tested = Cell::new(0);
    let open = |point: Vec3| {
        if point != Vec3::ZERO {
            tested.set(tested.get() + 1);
        }
        point.x < Num::ONE
    };
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = moving([1.5, 0.0]);
    wheel.drive(
        steer(point(0.0, -100.0), 100.0, 90.0),
        &mut stance,
        &mut drive,
        open,
    );
    assert_eq!(tested.get(), 2);
    assert_eq!(stance.facing, facing(90.0));
    assert_eq!(drive.velocity, [num(1.125), Num::ZERO]);
    // Within 15° of its goal, it looks at no arc.
    let tested = Cell::new(0);
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = moving([1.5, 0.0]);
    wheel.drive(
        steer(point(100.0, 0.0), 100.0, 90.0),
        &mut stance,
        &mut drive,
        |point| {
            if point != Vec3::ZERO {
                tested.set(tested.get() + 1);
            }
            point == Vec3::ZERO
        },
    );
    assert_eq!(tested.get(), 0);
}

#[test]
fn a_wheel_backs_to_a_goal_behind_it_and_turns_in_three_points_past_its_distance() {
    let wheel = mover(LocomotorAppearance::FourWheels, |locomotor| {
        locomotor.backwards = true;
        locomotor.three_point = Num::int(75);
    });
    // From a stop, its goal 50 m behind it, within its 75: it backs straight, its back toward the
    // goal, gaining 1.5 m a tick backwards, facing east all along.
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive::default();
    let goal = point(-50.0, 0.0);
    for (tick, x) in [(0, -1.5), (1, -4.5)] {
        let on_path = stance.at.ground().distance(goal);
        let steer = Steer {
            on_path,
            ..steer(goal, 0.0, 90.0)
        };
        wheel.drive(steer, &mut stance, &mut drive, |_| true);
        assert_eq!(stance.at, point(x, 0.0), "{tick}");
    }
    assert_eq!(stance.facing, facing(0.0));
    assert!(drive.backwards && !drive.three_point);
    // Its goal 100 m behind it: past 9°, it backs out at its turning speed, 0.75, not turning
    // from a stop; then it backs and turns toward the goal, 90° at once at its turning speed,
    // and the friction takes a quarter of its lateral velocity.
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive::default();
    let goal = point(-100.0, 0.0);
    wheel.drive(steer(goal, 100.0, 90.0), &mut stance, &mut drive, |_| true);
    assert_eq!((stance.at, stance.facing), (point(-0.75, 0.0), facing(0.0)));
    assert!(drive.backwards && drive.three_point);
    wheel.drive(steer(goal, 99.25, 90.0), &mut stance, &mut drive, |_| true);
    assert_eq!(stance.facing, facing(90.0));
    assert_eq!(drive.velocity, [num(-0.5625), Num::ZERO]);
    // A wheel that cannot back turns toward its goal behind it instead.
    let forward_only = mover(LocomotorAppearance::FourWheels, |_| {});
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive::default();
    forward_only.drive(steer(goal, 100.0, 90.0), &mut stance, &mut drive, |_| true);
    assert!(!drive.backwards);
    assert_eq!(drive.velocity, [num(0.75), Num::ZERO]);
}

#[test]
fn a_wheel_brakes_once_it_drove_near_its_goal_for_its_donut_wait() {
    // 30 m from its route's end, at 3 m a tick, its slow-down is (3 / 1.5 + 1) × 2 + 3 = 9 m:
    // far enough to drive on. Its fifth tick near the goal is still within its wait of 5; its
    // sixth is past it, and it brakes.
    let wheel = mover(LocomotorAppearance::FourWheels, |_| {});
    let braking_after = |donut| {
        let mut stance = Stance {
            at: Vec3::ZERO,
            facing: facing(0.0),
        };
        let mut drive = ZeroHourDrive {
            donut,
            ..moving([3.0, 0.0])
        };
        wheel.drive(
            steer(point(30.0, 0.0), 30.0, 90.0),
            &mut stance,
            &mut drive,
            |_| true,
        );
        (drive.donut, drive.braking)
    };
    assert_eq!(braking_after(4), (5, false));
    assert_eq!(braking_after(5), (6, true));
    // Past the donut distance its wait begins again.
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    let mut drive = ZeroHourDrive {
        donut: 6,
        ..moving([3.0, 0.0])
    };
    wheel.drive(
        steer(point(50.0, 0.0), 50.0, 90.0),
        &mut stance,
        &mut drive,
        |_| true,
    );
    assert_eq!((drive.donut, drive.braking), (0, false));
}

#[test]
fn a_stopped_drive_keeps_its_braked_mark_and_lands_as_it_starts() {
    // A mover that braked as it stopped lands toward its next goal as it starts again, at least
    // a cell a second, 5 m a tick, no farther than the goal; as it brakes no more, its push of
    // 1.5 then moves it on, to 6.5 m.
    let tread = mover(LocomotorAppearance::Treads, |_| {});
    let drive = ZeroHourDrive {
        braking: true,
        braked: true,
        donut: 3,
        ..moving([1.5, 0.0])
    };
    let stopped = drive.stopped();
    assert_eq!(
        stopped,
        ZeroHourDrive {
            braked: true,
            ..ZeroHourDrive::default()
        }
    );
    let mut drive = stopped;
    let mut stance = Stance {
        at: Vec3::ZERO,
        facing: facing(0.0),
    };
    tread.drive(
        steer(point(100.0, 0.0), 100.0, 90.0),
        &mut stance,
        &mut drive,
        |_| true,
    );
    assert_eq!(stance.at, point(6.5, 0.0));
    assert!(!drive.braked);
}

#[test]
fn a_mover_in_a_cell_it_may_not_stand_in_pushes_itself_out_but_for_one_that_corrects_none() {
    // Walls west of x = 0 and the mover a meter into them, facing north to a goal there: the
    // three cells a cell west of it are walled, so it pushes itself east by 3 / 5 m a tick each
    // tick, to the nearest bit, and drives no more that tick.
    let tread = mover(LocomotorAppearance::Treads, |_| {});
    let open = |point: Vec3| point.x >= Num::ZERO;
    let three_fifths = Num::int(3) / 5;
    let pushed = |velocity: [f64; 2]| {
        let mut stance = Stance {
            at: point(-1.0, 0.0),
            facing: facing(90.0),
        };
        let mut drive = moving(velocity);
        tread.drive(
            steer(point(-1.0, -100.0), 100.0, 90.0),
            &mut stance,
            &mut drive,
            open,
        );
        (stance.at.x, drive.velocity)
    };
    // From a stop, it gains the push alone; north it has no lateral velocity to lose.
    assert_eq!(
        pushed([0.0, 0.0]),
        (num(-1.0) + three_fifths, [three_fifths, Num::ZERO])
    );
    // Driving west into the wall at 1 m a tick, it gains as much again as the root of that, 1,
    // and loses a quarter of the velocity across its facing: −1 + 1 + 3/5 + 1/4 of 1.
    let into = pushed([-1.0, 0.0]);
    assert_eq!(
        into.1[0],
        num(-1.0) + Num::ONE + three_fifths + Num::QUARTER
    );
    // Leaving east faster than a quarter meter a tick, it drives as it would.
    let mut stance = Stance {
        at: point(-1.0, 0.0),
        facing: facing(90.0),
    };
    let mut drive = moving([0.5, 0.0]);
    let goal = point(-1.0, -100.0);
    tread.drive(steer(goal, 100.0, 90.0), &mut stance, &mut drive, open);
    let mut free = Stance {
        at: point(-1.0, 0.0),
        facing: facing(90.0),
    };
    let mut free_drive = moving([0.5, 0.0]);
    tread.drive(steer(goal, 100.0, 90.0), &mut free, &mut free_drive, |_| {
        true
    });
    assert_eq!((stance, drive), (free, free_drive));
    // A dozer's locomotor corrects none, and drives as it would in the open.
    let dozer = mover(LocomotorAppearance::Treads, |locomotor| {
        locomotor.corrects = false;
    });
    let mut stance = Stance {
        at: point(-1.0, 0.0),
        facing: facing(90.0),
    };
    let mut drive = ZeroHourDrive::default();
    dozer.drive(steer(goal, 100.0, 90.0), &mut stance, &mut drive, open);
    assert_eq!(stance.at, point(-1.0, -1.5));
}
