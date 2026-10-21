use std::num::NonZeroI64;

use campfire_actions::ProjectileFlight;
use campfire_common::Ticks;
use campfire_declarations::PerType;
use campfire_geometry::Radius;
use campfire_math::{Num, Rounding, SinCos};
use campfire_script::{Filters, TagNames, UnknownName};
use campfire_sim::MatchRate;

use crate::projectile_data::{CruiseData, CurveData, ProjectileData, ProjectileHits};

/// A projectile type as a match runs it: its speed a tick, half its width, the radius of the
/// circle it flies as, its range if it has its own, whether it homes, stops at its first hit, or
/// hits a unit once a cast, the units it hits, by no filter when it hits nothing, and its curve or
/// its steer, if it flies one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectileSpec {
    pub(crate) speed: Num,
    pub(crate) half_width: Radius,
    pub(crate) range: Option<Num>,
    pub homing: bool,
    pub(crate) stop_on_hit: bool,
    pub(crate) once_per_cast: bool,
    pub(crate) hits: Filters,
    pub(crate) curve: Option<CurveSpec>,
    pub(crate) steer: Option<SteerSpec>,
}

/// A steer as a match runs it (04-capabilities/actions.md, A projectile may be steered): its launch
/// speed a tick, the speed it gains each tick, its turn a tick as the sine and the cosine of its
/// angle, none for a turn of half a turn or more, which turns it any way, its straight run, its
/// lock at a unit and at a point and its cruise in meters, its fuel in ticks, rounded up, and
/// whether it tracks its target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SteerSpec {
    pub(crate) launch: Num,
    pub(crate) acceleration: Num,
    pub(crate) turn: Option<SinCos>,
    pub(crate) straight: Num,
    pub(crate) lock: Num,
    pub(crate) point_lock: Num,
    pub(crate) fuel: Ticks,
    pub(crate) cruise: Option<CruiseData>,
    pub(crate) tracks: bool,
}

/// A curve as a match runs it: its inner points, `[fraction, height]` each, how far its end
/// follows its target a tick, and its life in ticks, rounded up, if it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CurveSpec {
    pub(crate) first: [Num; 2],
    pub(crate) second: [Num; 2],
    pub(crate) follow: Num,
    pub(crate) life: Option<Ticks>,
}

impl ProjectileSpec {
    /// The spec of `data`: its speed a tick at `rate`, and what it hits among the tags of
    /// `tags`; an error for a filter of a tag `tags` lacks.
    pub fn of(
        data: &ProjectileData,
        tags: &TagNames,
        rate: MatchRate,
    ) -> Result<Self, UnknownName> {
        let hits = match &data.hits {
            ProjectileHits::Nothing => Filters::default(),
            ProjectileHits::Units(filters) => Filters::resolve(filters, tags)?,
        };
        let per_tick = |speed: Num| {
            speed
                .checked_div_int(NonZeroI64::from(rate.hz()).get(), Rounding::NearestEven)
                .expect("a speed over a tick rate fits")
        };
        let curve = data.curve.map(|curve: CurveData| CurveSpec {
            first: curve.first,
            second: curve.second,
            follow: per_tick(curve.follow),
            life: curve.life_ms.map(|ms| {
                rate.duration(ms.into())
                    .expect("a life of 32 bits of milliseconds counts in ticks")
            }),
        });
        let hz = NonZeroI64::from(rate.hz()).get();
        let steer = data.steer.map(|steer| {
            let degrees = steer
                .turn_rate
                .checked_div_int(hz, Rounding::NearestEven)
                .expect("a turn rate over a tick rate fits");
            SteerSpec {
                launch: per_tick(steer.launch_speed.unwrap_or(data.speed)),
                acceleration: steer
                    .acceleration
                    .checked_div_int(hz * hz, Rounding::NearestEven)
                    .expect("an acceleration over a tick rate squared fits"),
                turn: (degrees < Num::int(180)).then(|| SinCos::of_degrees(degrees)),
                straight: steer.straight,
                lock: steer.lock,
                point_lock: steer.point_lock.unwrap_or(steer.lock),
                fuel: rate
                    .duration(steer.fuel_ms.into())
                    .expect("a fuel of 32 bits of milliseconds counts in ticks"),
                cruise: steer.cruise,
                tracks: steer.tracks,
            }
        });
        Ok(Self {
            speed: per_tick(data.speed),
            half_width: Radius::new(data.width / 2)
                .expect("the decode bounds a projectile's width"),
            range: data.range,
            homing: data.homing,
            stop_on_hit: data.stop_on_hit,
            once_per_cast: data.once_per_cast,
            hits,
            curve,
            steer,
        })
    }

    /// How it flies.
    pub const fn flight(&self) -> ProjectileFlight {
        match (self.homing, self.curve, self.steer) {
            (true, ..) => ProjectileFlight::Homing,
            (false, Some(_), _) => ProjectileFlight::Curve,
            (false, None, Some(_)) => ProjectileFlight::Steered,
            (false, None, None) => ProjectileFlight::Line,
        }
    }
}

impl PerType for ProjectileSpec {
    const SCRIPTS_ONLY: bool = false;
}
