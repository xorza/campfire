use std::num::NonZeroI64;

use campfire_actions::ProjectileFlight;
use campfire_common::Ticks;
use campfire_declare::PerType;
use campfire_geometry::Radius;
use campfire_math::{Num, Rounding};
use campfire_script::{Filters, TagNames, UnknownName};
use campfire_sim::TickRate;

use crate::projectile_data::{CurveData, ProjectileData, ProjectileHits};

/// A projectile type as a match runs it: its speed a tick, half its width, the radius of the
/// circle it flies as, its range if it has its own, whether it homes, stops at its first hit, or
/// hits a unit once a cast, the units it hits, by no filter when it hits nothing, and its curve,
/// if it flies one.
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
    pub fn of(data: &ProjectileData, tags: &TagNames, rate: TickRate) -> Result<Self, UnknownName> {
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
        })
    }

    /// How it flies.
    pub const fn flight(&self) -> ProjectileFlight {
        match (self.homing, self.curve) {
            (true, _) => ProjectileFlight::Homing,
            (false, Some(_)) => ProjectileFlight::Curve,
            (false, None) => ProjectileFlight::Line,
        }
    }
}

impl PerType for ProjectileSpec {
    const SCRIPTS_ONLY: bool = false;
}
