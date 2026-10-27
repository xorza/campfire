use std::f32::consts::{PI, TAU};
use std::time::Duration;

use bevy::ecs::component::Component;
use bevy::math::Vec3;

/// Where a drawing's root stands on the ground and the way it turns, between two of its unit's.
#[derive(Component, Debug, Clone, Copy, PartialEq)]
pub(crate) enum Glide {
    /// A unit the client predicts, which moves in each tick the client runs: its spot after the
    /// tick before last and after the last tick. It is drawn one tick late, between the two, by the
    /// share of the next tick that passed, as the network stack's frame interpolation draws: exact
    /// at any speed, at a pause, and after a rollback, which runs the ticks again.
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
pub(crate) struct FrameClock {
    pub(crate) timestep: Duration,
    pub(crate) fraction: f32,
}

impl Follows {
    /// What a drawing follows of a unit the client only receives, if `received`.
    pub(crate) const fn of(received: bool) -> Self {
        if received {
            Self::Receipts
        } else {
            Self::Ticks
        }
    }
}

impl Spot {
    /// The spot `share` of the way from it to `to`: the place along the line, the yaw the
    /// shorter way round.
    pub(crate) fn lerp(self, to: Self, share: f32) -> Self {
        let turn = (to.yaw - self.yaw + PI).rem_euclid(TAU) - PI;
        Self {
            point: self.point.lerp(to.point, share),
            yaw: self.yaw + turn * share,
        }
    }
}

impl Glide {
    /// A drawing at rest at `spot` that follows `follows`: a received unit's from `time` of app
    /// time.
    pub(crate) const fn rest(spot: Spot, follows: Follows, time: Duration) -> Self {
        match follows {
            Follows::Ticks => Self::ticked(spot),
            Follows::Receipts => Self::timed(spot, time),
        }
    }

    /// A predicted unit's drawing at rest at `spot`.
    pub(crate) const fn ticked(spot: Spot) -> Self {
        Self::Ticked {
            before: spot,
            last: spot,
        }
    }

    /// A received unit's drawing at rest at `spot`, from `since` of app time.
    pub(crate) const fn timed(spot: Spot, since: Duration) -> Self {
        Self::Timed {
            from: spot,
            to: spot,
            since,
        }
    }

    /// Takes the predicted unit's spot after a tick. A drawing that glided as a received unit's
    /// starts at `spot`, as the unit became predicted.
    pub(crate) const fn tick(&mut self, spot: Spot) {
        *self = match *self {
            Self::Ticked { last, .. } => Self::Ticked {
                before: last,
                last: spot,
            },
            Self::Timed { .. } => Self::ticked(spot),
        };
    }

    /// Starts a glide from `drawn`, where the drawing stands, to the received unit's new spot
    /// `to`, at `now` of app time.
    pub(crate) const fn head(&mut self, drawn: Spot, to: Spot, now: Duration) {
        *self = Self::Timed {
            from: drawn,
            to,
            since: now,
        };
    }

    /// The meters a second its drawing walks at `now` of app time, on `clock`, as
    /// `PhysicsBehavior::getVelocityMagnitude` gives a unit's: its velocity's length.
    pub(crate) fn speed(&self, now: Duration, clock: FrameClock) -> f32 {
        self.velocity(now, clock).length()
    }

    /// The velocity its drawing moves at at `now` of app time, on `clock`, in meters a second: a
    /// predicted unit's step of its last tick; a received unit's step to its new spot while it
    /// glides there, none after.
    pub(crate) fn velocity(&self, now: Duration, clock: FrameClock) -> Vec3 {
        if clock.timestep.is_zero() {
            return Vec3::ZERO;
        }
        let (from, to) = match *self {
            Self::Ticked { before, last } => (before, last),
            Self::Timed { from, to, since } => {
                if now.saturating_sub(since) > clock.timestep {
                    return Vec3::ZERO;
                }
                (from, to)
            }
        };
        (to.point - from.point) / clock.timestep.as_secs_f32()
    }

    /// Where it stands at `now` of app time, on `clock`. A timed glide takes the share of the way
    /// that the time since its start is of a tick, at most all of it; only the difference of two
    /// times becomes an `f32`, so a long session loses no precision.
    pub(crate) fn at(&self, now: Duration, clock: FrameClock) -> Spot {
        match *self {
            Self::Ticked { before, last } => before.lerp(last, clock.fraction.clamp(0.0, 1.0)),
            Self::Timed { from, to, since } => {
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
mod tests;
