use std::time::Duration;

use bevy::ecs::component::Component;
use bevy::math::Vec3;

/// Where a drawing's root stands on the ground, between two places of its unit.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) enum Glide {
    /// A unit the client predicts, which moves in each tick the client runs: its place after the
    /// tick before last and after the last tick. It is drawn one tick late, between the two, by
    /// the share of the next tick that passed, as Lightyear's frame interpolation draws: exact at
    /// any speed, at a pause, and after a rollback, which runs the ticks again.
    Ticked { before: Vec3, last: Vec3 },
    /// A unit the client only receives: from where it was drawn when the unit's sim place last
    /// changed, to that place, over one tick from `since`, in app time.
    Timed {
        from: Vec3,
        to: Vec3,
        since: Duration,
    },
}

/// The fixed clock a frame is drawn at: how long a tick lasts now, and the share of the next
/// tick that passed.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TickClock {
    pub(crate) timestep: Duration,
    pub(crate) fraction: f32,
}

impl Glide {
    /// A predicted unit's drawing at rest `at`.
    pub(crate) const fn ticked(at: Vec3) -> Glide {
        Glide::Ticked {
            before: at,
            last: at,
        }
    }

    /// A received unit's drawing at rest `at`, from `since` of app time.
    pub(crate) const fn timed(at: Vec3, since: Duration) -> Glide {
        Glide::Timed {
            from: at,
            to: at,
            since,
        }
    }

    /// Takes the predicted unit's place `at` after a tick. A drawing that glided as a received
    /// unit's starts at `at`, as the unit became predicted.
    pub(crate) const fn tick(&mut self, at: Vec3) {
        *self = match *self {
            Glide::Ticked { last, .. } => Glide::Ticked {
                before: last,
                last: at,
            },
            Glide::Timed { .. } => Glide::ticked(at),
        };
    }

    /// Starts a glide from `drawn`, where the drawing stands, to the received unit's new place
    /// `to`, at `now` of app time.
    pub(crate) const fn head(&mut self, drawn: Vec3, to: Vec3, now: Duration) {
        *self = Glide::Timed {
            from: drawn,
            to,
            since: now,
        };
    }

    /// Where it stands at `now` of app time, on `clock`. A timed glide takes the share of the way
    /// that the time since its start is of a tick, at most all of it; only the difference of two
    /// times becomes an `f32`, so a long session loses no precision.
    pub(crate) fn at(&self, now: Duration, clock: TickClock) -> Vec3 {
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

    #[test]
    fn a_timed_glide_takes_one_tick_of_the_tick_it_is_read_at() {
        // From x = 0 to 2 from 10 h of app time: half way after half a tick of 50 ms, and all of
        // it after one; at a tick of 100 ms, as a match at half speed runs, a quarter of the way.
        let start = Duration::from_secs(36_000);
        let mut glide = Glide::timed(Vec3::ONE, Duration::ZERO);
        glide.head(Vec3::ZERO, Vec3::new(2.0, 0.0, 0.0), start);
        let at = |ms, tick_ms| {
            let clock = TickClock {
                timestep: Duration::from_millis(tick_ms),
                fraction: 0.9,
            };
            glide.at(start + Duration::from_millis(ms), clock).x
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
        assert_eq!(glide.at(Duration::ZERO, still).x, 2.0);
    }

    #[test]
    fn a_ticked_glide_draws_one_tick_late_by_the_share_of_the_next() {
        // Ticks put the unit at x = 1, then 3: drawn 0.25 of the way from 1 to 3, 1.5, whatever
        // the time; the app clock does not count.
        let mut glide = Glide::timed(Vec3::ZERO, Duration::ZERO);
        glide.tick(Vec3::X);
        assert_eq!(glide, Glide::ticked(Vec3::X));
        glide.tick(Vec3::X * 3.0);
        let clock = |fraction| TickClock {
            timestep: Duration::from_millis(50),
            fraction,
        };
        for now in [Duration::ZERO, Duration::from_secs(36_000)] {
            assert_eq!(glide.at(now, clock(0.25)).x, 1.5);
        }
        // A share past a whole tick, which a frame long enough to catch up may give, stops at
        // the last place.
        assert_eq!(glide.at(Duration::ZERO, clock(1.5)).x, 3.0);
        // A tick that does not move it brings it to rest at its last place.
        glide.tick(Vec3::X * 3.0);
        assert_eq!(glide.at(Duration::ZERO, clock(0.0)).x, 3.0);
    }
}
