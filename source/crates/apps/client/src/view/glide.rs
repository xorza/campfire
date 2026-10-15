use std::f32::consts::{PI, TAU};
use std::time::Duration;

use bevy::ecs::component::Component;
use bevy::math::Vec3;

/// Where a drawing's root stands on the ground and the way it turns, between two of its unit's.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) enum Glide {
    /// A unit the client predicts, which moves in each tick the client runs: its spot after the
    /// tick before last and after the last tick. It is drawn one tick late, between the two, by
    /// the share of the next tick that passed, as Lightyear's frame interpolation draws: exact at
    /// any speed, at a pause, and after a rollback, which runs the ticks again.
    Ticked { before: Spot, last: Spot },
    /// A unit the client only receives: from where it was drawn when the unit's sim place or
    /// facing last changed, to that spot, over one tick from `since`, in app time.
    Timed {
        from: Spot,
        to: Spot,
        since: Duration,
    },
}

/// What a unit's drawing follows: the ticks the client runs, for a unit it predicts, or each spot
/// the client receives, for one it only receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Follows {
    Ticks,
    Receipts,
}

/// A drawing's place and the way it turns about the vertical, in radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Spot {
    pub(crate) point: Vec3,
    pub(crate) yaw: f32,
}

/// The fixed clock a frame is drawn at: how long a tick lasts now, and the share of the next
/// tick that passed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TickClock {
    pub(crate) timestep: Duration,
    pub(crate) fraction: f32,
}

impl Follows {
    /// What a drawing follows of a unit the client only receives, if `received`.
    pub(crate) const fn of(received: bool) -> Follows {
        if received {
            Follows::Receipts
        } else {
            Follows::Ticks
        }
    }
}

impl Spot {
    /// The spot `share` of the way from it to `to`: the place along the line, the yaw the
    /// shorter way round.
    pub(crate) fn lerp(self, to: Spot, share: f32) -> Spot {
        let turn = (to.yaw - self.yaw + PI).rem_euclid(TAU) - PI;
        Spot {
            point: self.point.lerp(to.point, share),
            yaw: self.yaw + turn * share,
        }
    }
}

impl Glide {
    /// A drawing at rest at `spot` that follows `follows`: a received unit's from `time` of app
    /// time.
    pub(crate) const fn rest(spot: Spot, follows: Follows, time: Duration) -> Glide {
        match follows {
            Follows::Ticks => Glide::ticked(spot),
            Follows::Receipts => Glide::timed(spot, time),
        }
    }

    /// A predicted unit's drawing at rest at `spot`.
    pub(crate) const fn ticked(spot: Spot) -> Glide {
        Glide::Ticked {
            before: spot,
            last: spot,
        }
    }

    /// A received unit's drawing at rest at `spot`, from `since` of app time.
    pub(crate) const fn timed(spot: Spot, since: Duration) -> Glide {
        Glide::Timed {
            from: spot,
            to: spot,
            since,
        }
    }

    /// Takes the predicted unit's spot after a tick. A drawing that glided as a received unit's
    /// starts at `spot`, as the unit became predicted.
    pub(crate) const fn tick(&mut self, spot: Spot) {
        *self = match *self {
            Glide::Ticked { last, .. } => Glide::Ticked {
                before: last,
                last: spot,
            },
            Glide::Timed { .. } => Glide::ticked(spot),
        };
    }

    /// Starts a glide from `drawn`, where the drawing stands, to the received unit's new spot
    /// `to`, at `now` of app time.
    pub(crate) const fn head(&mut self, drawn: Spot, to: Spot, now: Duration) {
        *self = Glide::Timed {
            from: drawn,
            to,
            since: now,
        };
    }

    /// Where it stands at `now` of app time, on `clock`. A timed glide takes the share of the way
    /// that the time since its start is of a tick, at most all of it; only the difference of two
    /// times becomes an `f32`, so a long session loses no precision.
    pub(crate) fn at(&self, now: Duration, clock: TickClock) -> Spot {
        match *self {
            Glide::Ticked { before, last } => before.lerp(last, clock.fraction.clamp(0.0, 1.0)),
            Glide::Timed { from, to, since } => {
                if clock.timestep.is_zero() {
                    return to;
                }
                let done = now.saturating_sub(since).as_secs_f32() / clock.timestep.as_secs_f32();
                from.lerp(to, done.clamp(0.0, 1.0))
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
            let clock = TickClock {
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
        // A time before its start, as a clock never gives, holds it at its start; a tick of no
        // length puts it at its end.
        assert_eq!(at(0, 50), 0.0);
        let still = TickClock {
            timestep: Duration::ZERO,
            fraction: 0.0,
        };
        assert_eq!(glide.at(Duration::ZERO, still).point.x, 2.0);
    }

    #[test]
    fn a_ticked_glide_draws_one_tick_late_by_the_share_of_the_next() {
        // Ticks put the unit at x = 1, then 3: drawn 0.25 of the way from 1 to 3, 1.5, whatever
        // the time; the app clock does not count.
        let mut glide = Glide::timed(at_rest(Vec3::ZERO), Duration::ZERO);
        glide.tick(at_rest(Vec3::X));
        assert_eq!(glide, Glide::ticked(at_rest(Vec3::X)));
        glide.tick(at_rest(Vec3::X * 3.0));
        let clock = |fraction| TickClock {
            timestep: Duration::from_millis(50),
            fraction,
        };
        for now in [Duration::ZERO, Duration::from_secs(36_000)] {
            assert_eq!(glide.at(now, clock(0.25)).point.x, 1.5);
        }
        // A share past a whole tick, which a frame long enough to catch up may give, stops at
        // the last place.
        assert_eq!(glide.at(Duration::ZERO, clock(1.5)).point.x, 3.0);
        // A tick that does not move it brings it to rest at its last place.
        glide.tick(at_rest(Vec3::X * 3.0));
        assert_eq!(glide.at(Duration::ZERO, clock(0.0)).point.x, 3.0);
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
}
