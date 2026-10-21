use std::ops::ControlFlow;

use bevy_ecs::entity::Entity;
use campfire_actions::{Reached, Targets};
use campfire_common::StableNumber;
use campfire_deliveries::{AttackShot, Delivered, DeliveredPayload, Delivering, DeliveryQueue};
use campfire_geometry::{BodyGrid, Metric, Placed, Shape, Volume};
use campfire_math::{Num, Vec3};
use campfire_script::Filters;
use campfire_sim::{Position, StableId};
use campfire_values::{Hit, Team};

use crate::curve::Curve;
use crate::flight_state::FlightState;
use crate::projectile::{Flight, Payload, Projectile};
use crate::projectile_spec::ProjectileSpec;
use crate::struck_units::{Struck, StruckUnits};

/// The flights of a tick: where their hits and ends go, the units each line struck while it
/// flies, the bodies a line or a curve may meet, indexed as the stage began, and a scratch list
/// of the units a line's step meets, by share of the step.
#[derive(Debug)]
pub(crate) struct TickFlights<'a> {
    pub(crate) deliveries: &'a mut DeliveryQueue,
    pub(crate) struck: &'a mut StruckUnits,
    /// The units near, when a line or a curve flies.
    pub(crate) grid: Option<&'a BodyGrid<Entity, StableId>>,
    pub(crate) met: &'a mut Vec<(Num, StableId)>,
}

/// A curve a projectile flies: its first and its last control point as it launched, the unit it
/// aimed at, if one, and its count of points.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CurveFlight {
    from: Vec3,
    aimed: Option<StableId>,
    points: u32,
}

/// A steered flight as its step reads it: the unit it tracks, if one, and the point in space it
/// flies to when it tracks none.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SteeredFlight {
    tracks: Option<StableId>,
    to: Vec3,
}

/// A curve projectile's sphere where it stands: of radius `half` at the point in space `point`,
/// of a projectile of `team` from `source`, aimed at `aimed`, which meets what `hits` selects.
#[derive(Debug, Clone, Copy)]
struct Sphere<'a> {
    point: Vec3,
    half: Num,
    team: Team,
    source: StableId,
    aimed: Option<StableId>,
    hits: &'a Filters,
}

/// A projectile as it reaches a unit or its end: itself, and its team.
#[derive(Debug, Clone, Copy)]
struct Fired<'a> {
    projectile: &'a Projectile,
    team: Team,
}

/// A line a projectile flies: its origin, its unit direction and its range.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Line {
    from: Vec3,
    direction: Vec3,
    range: Num,
}

/// A projectile in flight this tick: its stable id, its team, its type's spec, where it is,
/// itself, and how far it flew.
#[derive(Debug)]
pub(crate) struct Aloft<'a> {
    pub id: StableId,
    pub team: Team,
    pub(crate) spec: &'a ProjectileSpec,
    pub(crate) position: &'a mut Position,
    pub(crate) projectile: &'a Projectile,
    pub(crate) state: &'a mut FlightState,
}

impl Aloft<'_> {
    /// What its hits are kept by: its cast for a type that strikes a unit once a cast, or
    /// itself.
    pub(crate) const fn strikes_by(&self) -> StableId {
        match self.projectile.payload() {
            Payload::Action { group, .. } if self.spec.once_per_cast => group,
            _ => self.id,
        }
    }
}

impl TickFlights<'_> {
    /// Flies `aloft` a step; whether it ended. A homing one flies towards its target's position
    /// now, and hits it where its step ends when its body, of half its width, then reaches the
    /// target's; one whose target is dead, gone, no target or lost to a teleport ends without a
    /// hit. One along a line
    /// hits each unit its type's `hits` selects whose body comes within half its width of this
    /// tick's path, in the order of the point of the path nearest each, then by stable id, each
    /// unit once, and once a cast for a type that says so; it ends at its first hit when its type
    /// stops on one, and at the end of its range. One on a curve flies as `curve` says, and a
    /// steered one as `steered` says.
    pub(crate) fn fly(&mut self, targets: &Targets<'_, '_>, aloft: Aloft<'_>) -> bool {
        match aloft.projectile.flight() {
            Flight::Homing { target } => self.is_homing(targets, aloft, target),
            Flight::Steered { tracks, to, .. } => {
                self.steered(targets, aloft, SteeredFlight { tracks, to })
            }
            Flight::Curve {
                from,
                aimed,
                points,
                ..
            } => self.curve(
                targets,
                aloft,
                CurveFlight {
                    from,
                    aimed,
                    points,
                },
            ),
            Flight::Line {
                from,
                direction,
                range,
                aimed,
            } => self.line(
                targets,
                aloft,
                Line {
                    from,
                    direction,
                    range,
                },
                aimed,
            ),
        }
    }

    fn is_homing(&mut self, targets: &Targets<'_, '_>, aloft: Aloft<'_>, target: StableId) -> bool {
        let Aloft {
            id,
            team,
            spec,
            position,
            projectile,
            state,
        } = aloft;
        let fired = Fired { projectile, team };
        let (flown, lost) = (state.flown(), state.is_lost());
        let from = *position;
        let Some(unit) = targets.living(target).filter(|_| !lost) else {
            let hit = Hit {
                delivery: Some(id.into()),
                target: Some(target.into()),
                position: from.get(),
                distance: flown,
                direction: None,
            };
            self.reach(fired, Reached::End, hit);
            return true;
        };
        let metric = targets.metric();
        let moved = metric.step_toward(from.get(), unit.position, spec.speed);
        let offset = metric.offset(from.get(), moved);
        let flown = flown + offset.length();
        if !metric.reaches(
            Placed::new(moved, Shape::Circle(spec.half_width)),
            Num::ZERO,
            Placed::new(unit.position, unit.shape),
        ) {
            *position = Position::from(moved);
            state.fly_to(flown);
            return false;
        }
        let hit = Hit {
            delivery: Some(id.into()),
            target: Some(target.into()),
            position: moved,
            distance: flown,
            direction: offset.normalized(),
        };
        self.reach(fired, Reached::Hit(target), hit);
        self.reach(fired, Reached::End, hit);
        true
    }

    /// Flies `aloft` a step along its line, on from the meters of its range it flew, to its
    /// place on the line from the line's origin; its hits and its end name `aimed` as their
    /// action's target.
    fn line(
        &mut self,
        targets: &Targets<'_, '_>,
        aloft: Aloft<'_>,
        Line {
            from: origin,
            direction,
            range,
        }: Line,
        aimed: Option<StableId>,
    ) -> bool {
        let by = aloft.strikes_by();
        let Aloft {
            id,
            team,
            spec,
            position,
            projectile,
            state,
        } = aloft;
        let fired = Fired { projectile, team };
        let flown = state.flown();
        let from = *position;
        let step = spec.speed.min(range - flown);
        let line = |position, distance| Hit {
            delivery: Some(id.into()),
            target: aimed.map(StableNumber::from),
            position,
            distance,
            direction: Some(direction),
        };
        let along = |distance: Num| {
            let offset = direction.checked_mul(distance)?;
            origin.checked_add(offset)
        };
        let to = along(flown + step);
        let Some(to) = to.filter(|&to| to.within_bound()) else {
            self.reach(fired, Reached::End, line(from.get(), flown));
            return true;
        };
        let metric = targets.metric();
        self.met.clear();
        if !spec.hits.is_empty() {
            let hits = &spec.hits;
            let (start, end) = (from.get(), to);
            let half = spec.half_width.get();
            let low = [start.x.min(end.x), start.z.min(end.z)]
                .map(|axis| axis.checked_sub(half).unwrap_or(Num::MIN));
            let high = [start.x.max(end.x), start.z.max(end.z)]
                .map(|axis| axis.checked_add(half).unwrap_or(Num::MAX));
            let (met, struck) = (&mut *self.met, &*self.struck);
            let grid = self.grid.expect("a line finds the units near");
            grid.visit(low, high, |body| {
                let Some(unit) = targets.target_at(body.key) else {
                    return ControlFlow::Continue(());
                };
                let selects = hits.selects(targets.relation(team, unit.team), unit.tags);
                if !selects || struck.contains(Struck { by, id: unit.id }) {
                    return ControlFlow::Continue(());
                }
                if let Some(place) =
                    metric.meets(from.get(), to, Placed::new(unit.position, unit.shape), half)
                {
                    met.push((place, unit.id));
                }
                ControlFlow::Continue(())
            });
        }
        // The places along the step order where it meets each unit, to the nearest bit, then the
        // unit's id. A place lies on the step as its end was rounded, so within a bit or two of
        // the step's length, and stops there.
        self.met.sort_unstable();
        for at in 0..self.met.len() {
            let (place, struck) = self.met[at];
            let travelled = place.min(step);
            let position = along(flown + travelled).expect("a point of a step within the bound");
            let hit = line(position, flown + travelled);
            self.struck.insert(Struck { by, id: struck });
            self.reach(fired, Reached::Hit(struck), hit);
            if spec.stop_on_hit {
                self.reach(fired, Reached::End, hit);
                return true;
            }
        }
        *position = Position::from(to);
        state.fly_to(flown + step);
        if flown + step < range {
            return false;
        }
        self.reach(fired, Reached::End, line(to, range));
        true
    }

    /// Flies `aloft` to its curve's next point (04-capabilities/actions.md, A projectile may fly a
    /// curve; 04-capabilities/actions.md, A curve meets units by its point). It ends where it is in
    /// the tick after its last point, and as its life passes. Before its step, when it aimed at a
    /// unit that lives and whose center lies more than √0.1 m from its end, its end moves toward
    /// that center by at most its type's `follow`, with its count of points kept. At its point, its
    /// sphere of half its width meets the volumes of the living units its type's `hits` selects,
    /// and of the unit it aimed at, never its source's: the one of the lowest stable id is its hit,
    /// and it ends there.
    fn curve(
        &mut self,
        targets: &Targets<'_, '_>,
        aloft: Aloft<'_>,
        CurveFlight {
            from,
            aimed,
            points,
        }: CurveFlight,
    ) -> bool {
        let Aloft {
            id,
            team,
            spec,
            position,
            projectile,
            state,
        } = aloft;
        let fired = Fired { projectile, team };
        let FlightState::Curve {
            flown,
            steps,
            mut end,
        } = *state
        else {
            panic!("a curve's state is a curve's");
        };
        let shape = spec.curve.expect("a curve's type has a curve");
        let map = targets.map();
        let hit = |position, distance, direction| Hit {
            delivery: Some(id.into()),
            target: aimed.map(StableNumber::from),
            position,
            distance,
            direction,
        };
        // Its life counts from its launch, the tick before its first point.
        let lived = u64::from(steps) + 1;
        if steps >= points || shape.life.is_some_and(|life| lived >= life.get()) {
            self.reach(fired, Reached::End, hit(position.get(), flown, None));
            return true;
        }
        let aimed_center = aimed
            .and_then(|aimed| targets.volume_of(aimed))
            .map(|volume| volume.center());
        if let Some(center) = aimed_center
            && shape.follow > Num::ZERO
            // More than √0.1 m away: 10 · d² past 1 m² in bits, 2⁴⁸.
            && 10 * (center - end).length_squared_bits() > 1 << 48
        {
            end = end.step_toward(center, shape.follow);
        }
        let curve = Curve::new(from, end, &shape, map.highest_along(from, end));
        let point = curve.point(steps, points);
        let here = map.point(position.get());
        let moved = map.position(point);
        if !moved.within_bound() {
            self.reach(fired, Reached::End, hit(position.get(), flown, None));
            return true;
        }
        let step = point - here;
        let flown = flown + step.length();
        *position = Position::from(moved);
        *state = FlightState::Curve {
            flown,
            steps: steps + 1,
            end,
        };
        let sphere = Sphere {
            point,
            half: spec.half_width.get(),
            team,
            source: projectile.source(),
            aimed,
            hits: &spec.hits,
        };
        let Some(met) = self.met(targets, sphere) else {
            return false;
        };
        let hit = hit(moved, flown, step.normalized());
        self.reach(fired, Reached::Hit(met), hit);
        self.reach(fired, Reached::End, hit);
        true
    }

    /// Flies `aloft` a step of its steer (04-capabilities/actions.md, A projectile may be steered).
    /// It ends with no hit when the unit it tracks lives no more or a teleport lost it, and where
    /// it stands as its fuel runs out. Its speed gains its acceleration up to its type's speed.
    /// Within its lock of its goal on the ground, its tracked unit's center, or within its point's
    /// lock of its point, it steps straight at it; else it turns toward its goal, raised to its
    /// cruise height over the goal's ground until its dive, by at most its turn, none within its
    /// straight run, and steps along its heading. It ends where it leaves the world's bound, or
    /// meets the ground of a planar map. At its point, its sphere of half its width meets the
    /// volumes of the living units its type's `hits` selects, and of the unit it tracks, never its
    /// source's: the one of the lowest stable id is its hit, and it ends there; one that tracks no
    /// unit ends at its point.
    fn steered(
        &mut self,
        targets: &Targets<'_, '_>,
        aloft: Aloft<'_>,
        SteeredFlight { tracks, to }: SteeredFlight,
    ) -> bool {
        let Aloft {
            id,
            team,
            spec,
            position,
            projectile,
            state,
        } = aloft;
        let fired = Fired { projectile, team };
        let FlightState::Steered {
            flown,
            steps,
            speed,
            heading,
            lost,
        } = *state
        else {
            panic!("a steered flight's state is a steered one's");
        };
        let steer = spec.steer.expect("a steered type has a steer");
        let map = targets.map();
        let hit = |position, distance, direction| Hit {
            delivery: Some(id.into()),
            target: tracks.map(StableNumber::from),
            position,
            distance,
            direction,
        };
        let (goal, lock) = match tracks {
            Some(target) => (
                targets
                    .volume_of(target)
                    .filter(|_| !lost)
                    .map(|volume| volume.center()),
                steer.lock,
            ),
            None => (Some(to), steer.point_lock),
        };
        let Some(goal) = goal.filter(|_| u64::from(steps) < steer.fuel.get()) else {
            self.reach(fired, Reached::End, hit(position.get(), flown, None));
            return true;
        };
        let here = map.point(position.get());
        let speed = (speed + steer.acceleration).min(spec.speed);
        let (point, heading) = if here.within_ground(goal, lock) {
            (
                here.step_toward(goal, speed),
                heading.turned(goal - here, None),
            )
        } else {
            let aim = match steer.cruise {
                Some(cruise) if !here.within_ground(goal, cruise.dive) => {
                    map.point(Vec3::new(goal.x, cruise.height, goal.z))
                }
                _ => goal,
            };
            let heading = if flown < steer.straight {
                heading
            } else {
                heading.turned(aim - here, steer.turn)
            };
            (here + heading.get() * speed, heading)
        };
        let moved = map.position(point);
        let grounded = map.metric() == Metric::Planar && moved.y < Num::ZERO;
        if !moved.within_bound() || grounded {
            let stands = if grounded {
                Vec3::new(moved.x, Num::ZERO, moved.z)
            } else {
                position.get()
            };
            self.reach(fired, Reached::End, hit(stands, flown, None));
            return true;
        }
        let step = point - here;
        let flown = flown + step.length();
        *position = Position::from(moved);
        *state = FlightState::Steered {
            flown,
            steps: steps + 1,
            speed,
            heading,
            lost,
        };
        let sphere = Sphere {
            point,
            half: spec.half_width.get(),
            team,
            source: projectile.source(),
            aimed: tracks,
            hits: &spec.hits,
        };
        let direction = step.normalized();
        if let Some(met) = self.met(targets, sphere) {
            let hit = hit(moved, flown, direction);
            self.reach(fired, Reached::Hit(met), hit);
            self.reach(fired, Reached::End, hit);
            return true;
        }
        if tracks.is_none() && point == to {
            self.reach(fired, Reached::End, hit(moved, flown, direction));
            return true;
        }
        false
    }

    /// The unit of the lowest stable id whose volume `sphere` meets, touching included: a living
    /// unit its `hits` selects, or the unit it aimed at, never its source.
    fn met(&self, targets: &Targets<'_, '_>, sphere: Sphere<'_>) -> Option<StableId> {
        let Sphere {
            point,
            half,
            team,
            source,
            aimed,
            hits,
        } = sphere;
        let meets =
            |volume: Option<Volume>| volume.is_some_and(|volume| volume.reaches(point, half));
        let mut met = aimed.filter(|&aimed| aimed != source && meets(targets.volume_of(aimed)));
        if !hits.is_empty() {
            let grid = self.grid.expect("a curve finds the units near");
            grid.visit_near(point, half, |body| {
                let Some(unit) = targets.target_at(body.key) else {
                    return ControlFlow::Continue(());
                };
                let selects = hits.selects(targets.relation(team, unit.team), unit.tags);
                let lower = met.is_none_or(|met| unit.id < met);
                if selects && lower && unit.id != source && meets(targets.volume_at(body.key)) {
                    met = Some(unit.id);
                }
                ControlFlow::Continue(())
            });
        }
        met
    }

    /// What `fired` delivers as it reaches what `reach` says, as `hit` says: an attack's shot hit
    /// or end, with its damage and its roll, or its action's `on_hit` or `on_end`.
    fn reach(&mut self, fired: Fired<'_>, reach: Reached, hit: Hit) {
        let Fired { projectile, team } = fired;
        let source = projectile.source();
        match projectile.payload() {
            Payload::Attack(damage) => self.deliveries.deliver(Delivered {
                payload: DeliveredPayload::Attack(AttackShot {
                    source,
                    team,
                    damage,
                }),
                reach,
                hit,
            }),
            Payload::Action {
                action,
                rank,
                start,
                ..
            } => self.deliveries.deliver(Delivered {
                payload: DeliveredPayload::Action(Delivering {
                    source,
                    action,
                    rank,
                    start,
                    launch: None,
                }),
                reach,
                hit,
            }),
        }
    }
}
