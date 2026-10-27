use super::*;

/// A spot at `point`, turned none.
const fn at_rest(point: Vec3) -> Spot {
    Spot { point, yaw: 0.0 }
}

#[test]
fn a_timed_glide_takes_one_tick_of_the_tick_it_is_read_at() {
    // From x = 0 to 2 from 10 h of app time: half way after half a tick of 50 ms, and all of
    // it after one; at a tick of 100 ms, as a match at half speed runs, a quarter of the way.
    let start = Duration::from_secs(36_000);
    let mut glide = Glide::timed(at_rest(Vec3::ONE), Duration::ZERO);
    glide.head(
        at_rest(Vec3::ZERO),
        at_rest(Vec3::new(2.0, 0.0, 0.0)),
        start,
    );
    let at = |ms, tick_ms| {
        let clock = FrameClock {
            timestep: Duration::from_millis(tick_ms),
            fraction: 0.9,
        };
        glide.at(start + Duration::from_millis(ms), clock).point.x
    };
    assert_eq!(
        [at(0, 50), at(25, 50), at(50, 50), at(90, 50)],
        [0.0, 1.0, 2.0, 2.0]
    );
    assert_eq!(at(25, 100), 0.5);
    // It walks 2 m in a tick of 50 ms, 40 m a second, until the tick ends, then none; 20 at a
    // tick of 100 ms.
    let speed = |ms, tick_ms| {
        let clock = FrameClock {
            timestep: Duration::from_millis(tick_ms),
            fraction: 0.9,
        };
        glide.speed(start + Duration::from_millis(ms), clock)
    };
    assert_eq!(
        [speed(0, 50), speed(50, 50), speed(51, 50), speed(25, 100)],
        [40.0, 40.0, 0.0, 20.0]
    );
    let velocity = |ms| {
        let clock = FrameClock {
            timestep: Duration::from_millis(50),
            fraction: 0.9,
        };
        glide.velocity(start + Duration::from_millis(ms), clock)
    };
    assert_eq!([velocity(50), velocity(51)], [Vec3::X * 40.0, Vec3::ZERO]);
    // A time before its start, as a clock never gives, holds it at its start; a tick of no
    // length puts it at its end.
    assert_eq!(at(0, 50), 0.0);
    let still = FrameClock {
        timestep: Duration::ZERO,
        fraction: 0.0,
    };
    assert_eq!(glide.at(Duration::ZERO, still).point.x, 2.0);
    assert_eq!(glide.speed(Duration::ZERO, still), 0.0);
}

#[test]
fn a_ticked_glide_draws_one_tick_late_by_the_share_of_the_next() {
    // Ticks put the unit at x = 1, then 3: drawn 0.25 of the way from 1 to 3, 1.5, whatever
    // the time; the app clock does not count.
    let mut glide = Glide::timed(at_rest(Vec3::ZERO), Duration::ZERO);
    glide.tick(at_rest(Vec3::X));
    assert_eq!(glide, Glide::ticked(at_rest(Vec3::X)));
    glide.tick(at_rest(Vec3::X * 3.0));
    let clock = |fraction| FrameClock {
        timestep: Duration::from_millis(50),
        fraction,
    };
    for now in [Duration::ZERO, Duration::from_secs(36_000)] {
        assert_eq!(glide.at(now, clock(0.25)).point.x, 1.5);
        // Its last tick's step, 2 m along x in 50 ms: 40 m a second.
        assert_eq!(glide.speed(now, clock(0.25)), 40.0);
        assert_eq!(glide.velocity(now, clock(0.25)), Vec3::X * 40.0);
    }
    // A share past a whole tick, which a frame long enough to catch up may give, stops at
    // the last place.
    assert_eq!(glide.at(Duration::ZERO, clock(1.5)).point.x, 3.0);
    // A tick that does not move it brings it to rest at its last place.
    glide.tick(at_rest(Vec3::X * 3.0));
    assert_eq!(glide.at(Duration::ZERO, clock(0.0)).point.x, 3.0);
    assert_eq!(glide.speed(Duration::ZERO, clock(0.0)), 0.0);
}

#[test]
fn a_spot_turns_the_shorter_way_round() {
    // From 3π/2 to 0 is a quarter turn up through 2π, not three quarters down: halfway is
    // 7π/4; and from 0 to 3π/2 the same quarter, down, halfway −π/4.
    let turned = |yaw| Spot {
        point: Vec3::ZERO,
        yaw,
    };
    let three_quarters = 1.5 * PI;
    let half = turned(three_quarters).lerp(turned(0.0), 0.5).yaw;
    assert!((half - 1.75 * PI).abs() < 1e-6, "{half}");
    let back = turned(0.0).lerp(turned(three_quarters), 0.5).yaw;
    assert!((back + 0.25 * PI).abs() < 1e-6, "{back}");
    // All the way, the yaw of the end, a whole turn apart at most.
    let end = turned(three_quarters).lerp(turned(0.0), 1.0).yaw;
    assert!((end - TAU).abs() < 1e-6, "{end}");
}
