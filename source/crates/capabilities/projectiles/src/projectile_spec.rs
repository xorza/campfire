use std::num::NonZeroI64;

use campfire_common::Ticks;
use campfire_declarations::{BookCx, BookError, BookPlace, ById, ByIdBuilder, PerId};
use campfire_geometry::Radius;
use campfire_math::{Num, Rounding, SinCos};
use campfire_script::{Filters, TagNames, UnknownName};
use campfire_sim::MatchRate;
use campfire_units::UnitTypes;
use campfire_values::UnitType;

use crate::projectile_data::{CruiseData, ProjectileData, ProjectileHits};
use crate::projectile_flight::ProjectileFlight;

/// A projectile type as a match runs it: its speed a tick, half its width, the radius of the
/// circle it flies as, the units it hits, by no filter when it hits nothing, and how it flies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectileSpec {
    pub(crate) speed: Num,
    pub(crate) half_width: Radius,
    pub(crate) hits: Filters,
    pub(crate) flight: FlightSpec,
}

/// How a projectile type flies, each way with what only it takes: homing on a unit or along a
/// line, each straight, on a curve, or steered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlightSpec {
    Homing(StraightSpec),
    Line(StraightSpec),
    Curve(CurveSpec),
    Steered(SteerSpec),
}

/// What a straight flight, homing or along a line, takes: its range if it has its own, whether it
/// stops at its first hit, and whether it hits a unit once a cast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StraightSpec {
    pub(crate) range: Option<Num>,
    pub(crate) stop_on_hit: bool,
    pub(crate) once_per_cast: bool,
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
        let hz = NonZeroI64::from(rate.hz()).get();
        let straight = StraightSpec {
            range: data.range,
            stop_on_hit: data.stop_on_hit,
            once_per_cast: data.once_per_cast,
        };
        let flight = match (data.homing, data.curve, data.steer) {
            (true, ..) => FlightSpec::Homing(straight),
            (false, Some(curve), _) => FlightSpec::Curve(CurveSpec {
                first: curve.first,
                second: curve.second,
                follow: per_tick(curve.follow),
                life: curve.life_ms.map(|ms| {
                    rate.duration(ms.into())
                        .expect("a life of 32 bits of milliseconds counts in ticks")
                }),
            }),
            (false, None, Some(steer)) => {
                let degrees = steer
                    .turn_rate
                    .checked_div_int(hz, Rounding::NearestEven)
                    .expect("a turn rate over a tick rate fits");
                FlightSpec::Steered(SteerSpec {
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
                })
            }
            (false, None, None) => FlightSpec::Line(straight),
        };
        Ok(Self {
            speed: per_tick(data.speed),
            half_width: Radius::new(data.width / 2)
                .expect("the decode bounds a projectile's width"),
            hits,
            flight,
        })
    }

    /// Its speed a tick as it launches: a steered type's launch speed, any other's speed.
    pub const fn launch_speed(&self) -> Num {
        match self.flight {
            FlightSpec::Steered(steer) => steer.launch,
            FlightSpec::Homing(_) | FlightSpec::Line(_) | FlightSpec::Curve(_) => self.speed,
        }
    }

    /// How it flies.
    pub const fn flight(&self) -> ProjectileFlight {
        match self.flight {
            FlightSpec::Homing(_) => ProjectileFlight::Homing,
            FlightSpec::Line(_) => ProjectileFlight::Line,
            FlightSpec::Curve(_) => ProjectileFlight::Curve,
            FlightSpec::Steered(_) => ProjectileFlight::Steered,
        }
    }

    /// What its straight flight takes, homing or along a line; none for a curve or a steer.
    pub(crate) const fn straight(&self) -> Option<StraightSpec> {
        match self.flight {
            FlightSpec::Homing(straight) | FlightSpec::Line(straight) => Some(straight),
            FlightSpec::Curve(_) | FlightSpec::Steered(_) => None,
        }
    }

    /// Whether it homes on a unit.
    pub const fn homes(&self) -> bool {
        matches!(self.flight, FlightSpec::Homing(_))
    }

    /// Its own range of a straight flight; none for one of its action's range, a curve or a
    /// steer.
    pub(crate) const fn own_range(&self) -> Option<Num> {
        match self.straight() {
            Some(straight) => straight.range,
            None => None,
        }
    }

    /// Its curve, when it flies one.
    pub(crate) const fn curve(&self) -> Option<CurveSpec> {
        match self.flight {
            FlightSpec::Curve(curve) => Some(curve),
            _ => None,
        }
    }

    /// Its steer, when it is steered.
    pub(crate) const fn steer(&self) -> Option<SteerSpec> {
        match self.flight {
            FlightSpec::Steered(steer) => Some(steer),
            _ => None,
        }
    }
}

impl PerId<UnitType> for ProjectileSpec {
    const SCRIPTS_ONLY: bool = false;
}

impl ProjectileSpec {
    /// The spec of each projectile type, its filters resolved against the match's tags; an error
    /// for a tag the match does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ById<UnitType, Self>, BookError> {
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut book = ByIdBuilder::default();
        for unit in cx.types() {
            let Some(projectile) = unit.data.section::<ProjectileData>() else {
                continue;
            };
            let spec = Self::of(projectile, tags, cx.rate()).map_err(|name| {
                BookError::unknown(unit.package, BookPlace::UnitType(unit.place()), name)
            })?;
            book.set(unit.unit_type, spec);
        }
        Ok(book.build())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use campfire_codec::Toml;

    use super::*;

    #[test]
    fn a_projectile_launches_at_its_steers_launch_speed_or_else_at_its_speed() {
        // At 30 ticks a second, a missile of 30 m/s flies 1 m a tick: its steer of a launch speed
        // of 15 m/s leaves at ½ m a tick, one of none at the missile's speed, as a homing shell.
        let rate = MatchRate::new(NonZeroU32::new(30).unwrap());
        let launch = |text: &str| {
            let data = Toml::parse::<ProjectileData>(text).unwrap();
            let spec = ProjectileSpec::of(&data, &TagNames::default(), rate).unwrap();
            spec.launch_speed()
        };
        let steer = "turn_rate = 90, fuel_ms = 1000";
        assert_eq!(
            launch(&format!(
                "speed = 30\nsteer = {{ launch_speed = 15, {steer} }}"
            )),
            Num::HALF
        );
        assert_eq!(
            launch(&format!("speed = 30\nsteer = {{ {steer} }}")),
            Num::ONE
        );
        assert_eq!(launch("speed = 30\nhoming = true"), Num::ONE);
    }
}
