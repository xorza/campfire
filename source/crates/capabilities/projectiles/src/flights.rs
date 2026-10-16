use std::cmp::Ordering;
use std::ops::ControlFlow;

use bevy_ecs::entity::Entity;
use campfire_actions::{ShotHit, ShotHits, Targets};
use campfire_deliveries::{Delivered, Deliveries, Delivering, Reached};
use campfire_geometry::{BodyGrid, Placed, Shape, Volume};
use campfire_math::{Num, Vec3};
use campfire_script::Filter;
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
pub(crate) struct Flights<'a> {
    pub(crate) hits: &'a mut ShotHits,
    pub(crate) deliveries: &'a mut Deliveries,
    pub(crate) struck: &'a mut StruckUnits,
    /// The units near, when a line or a curve flies.
    pub(crate) grid: Option<&'a BodyGrid<Entity>>,
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

/// A curve projectile's sphere where it stands: of radius `half` at the point in space `point`,
/// of a projectile of `team` from `source`, aimed at `aimed`, which meets what `hits` selects.
#[derive(Debug, Clone, Copy)]
struct Sphere {
    point: Vec3,
    half: Num,
    team: Team,
    source: StableId,
    aimed: Option<StableId>,
    hits: Option<Filter>,
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
    pub(crate) spec: ProjectileSpec,
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

impl Flights<'_> {
    /// Flies `aloft` a step; whether it ended. A homing one flies towards its target's position
    /// now, and hits it where its step ends when its body, of half its width, then reaches the
    /// target's; one whose target is dead, gone, no target or lost to a teleport ends without a
    /// hit. One along a line
    /// hits each unit its type's `hits` selects whose body comes within half its width of this
    /// tick's path, in the order of the point of the path nearest each, then by stable id, each
    /// unit once, and once a cast for a type that says so; it ends at its first hit when its type
    /// stops on one, and at the end of its range. One on a curve flies as `curve` says.
    pub(crate) fn fly(&mut self, targets: &Targets<'_, '_>, aloft: Aloft<'_>) -> bool {
        match aloft.projectile.flight() {
            Flight::Homing { target } => self.homing(targets, aloft, target),
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

    fn homing(&mut self, targets: &Targets<'_, '_>, aloft: Aloft<'_>, target: StableId) -> bool {
        let Aloft {
            id,
            spec,
            position,
            projectile,
            state,
            ..
        } = aloft;
        let (flown, lost) = (state.flown(), state.lost());
        let from = *position;
        let Some(unit) = targets.living(target).filter(|_| !lost) else {
            let hit = Hit {
                delivery: Some(id),
                target: Some(target),
                pos: from.get(),
                distance: flown,
                direction: None,
            };
            self.end(projectile, hit);
            return true;
        };
        let metric = targets.metric();
        let moved = metric.step_toward(from.get(), unit.pos, spec.speed);
        let offset = metric.offset(from.get(), moved);
        let flown = flown + offset.length();
        if !metric.reaches(
            Placed::new(moved, Shape::Circle(spec.half_width)),
            Num::ZERO,
            Placed::new(unit.pos, unit.shape),
        ) {
            *position = Position::from(moved);
            state.fly_to(flown);
            return false;
        }
        let hit = Hit {
            delivery: Some(id),
            target: Some(target),
            pos: moved,
            distance: flown,
            direction: offset.normalized(),
        };
        self.strike(projectile, target, hit);
        self.end(projectile, hit);
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
        let flown = state.flown();
        let from = *position;
        let step = spec.speed.min(range - flown);
        let line = |pos, distance| Hit {
            delivery: Some(id),
            target: aimed,
            pos,
            distance,
            direction: Some(direction),
        };
        let along = |distance: Num| {
            let offset = direction.checked_mul(distance)?;
            origin.checked_add(offset)
        };
        let to = along(flown + step);
        let Some(to) = to.filter(|&to| Position::from(to).within_bound()) else {
            self.end(projectile, line(from.get(), flown));
            return true;
        };
        let metric = targets.metric();
        self.met.clear();
        if let Some(hits) = spec.hits {
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
                    metric.meets(from.get(), to, Placed::new(unit.pos, unit.shape), half)
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
            let pos = along(flown + travelled).expect("a point of a step within the bound");
            let hit = line(pos, flown + travelled);
            self.struck.insert(Struck { by, id: struck });
            self.strike(projectile, struck, hit);
            if spec.stop_on_hit {
                self.end(projectile, hit);
                return true;
            }
        }
        *position = Position::from(to);
        state.fly_to(flown + step);
        if flown + step < range {
            return false;
        }
        self.end(projectile, line(to, range));
        true
    }

    /// Flies `aloft` to its curve's next point (20-zero-hour-projectiles.md, A projectile may fly a
    /// curve; 20-zero-hour-projectiles.md, A curve meets units by its point). It ends where it is in the tick after its last
    /// point, and as its life passes. Before its step, when it aimed at a unit that lives and
    /// whose center lies more than √0.1 m from its end, its end moves toward that center by at
    /// most its type's `follow`, with its count of points kept. At its point, its sphere of half
    /// its width meets the volumes of the living units its type's `hits` selects, and of the unit
    /// it aimed at, never its source's: the one of the lowest stable id is its hit, and it ends
    /// there.
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
        let hit = |pos, distance, direction| Hit {
            delivery: Some(id),
            target: aimed,
            pos,
            distance,
            direction,
        };
        // Its life counts from its launch, the tick before its first point.
        let lived = u64::from(steps) + 1;
        if steps >= points || shape.life.is_some_and(|life| lived >= life.get()) {
            self.end(projectile, hit(position.get(), flown, None));
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
        if !Position::from(moved).within_bound() {
            self.end(projectile, hit(position.get(), flown, None));
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
            hits: spec.hits,
        };
        let Some(met) = self.met(targets, sphere) else {
            return false;
        };
        let hit = hit(moved, flown, step.normalized());
        self.strike(projectile, met, hit);
        self.end(projectile, hit);
        true
    }

    /// The unit of the lowest stable id whose volume `sphere` meets, touching included: a living
    /// unit its `hits` selects, or the unit it aimed at, never its source.
    fn met(&self, targets: &Targets<'_, '_>, sphere: Sphere) -> Option<StableId> {
        let Sphere {
            point,
            half,
            team,
            source,
            aimed,
            hits,
        } = sphere;
        let meets = |volume: Option<Volume>| {
            volume.is_some_and(|volume| volume.nearest(point, half) != Ordering::Greater)
        };
        let mut met = aimed.filter(|&aimed| aimed != source && meets(targets.volume_of(aimed)));
        if let Some(hits) = hits {
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

    /// What a hit of `target` delivers: an attack's shot hit, with its damage and its roll, or its
    /// action's `on_hit`.
    fn strike(&mut self, projectile: &Projectile, target: StableId, hit: Hit) {
        match projectile.payload() {
            Payload::Attack(damage) => self.hits.0.push(ShotHit {
                source: projectile.source(),
                target,
                damage,
                hit,
            }),
            Payload::Action {
                action,
                rank,
                start,
                ..
            } => self.deliveries.deliver(Delivered {
                by: Delivering {
                    source: projectile.source(),
                    action,
                    rank,
                    start,
                    launch: None,
                },
                reach: Reached::Hit(target),
                hit,
            }),
        }
    }

    /// An action's projectile ends as `hit` says: its `on_end` runs. An attack's ends with
    /// nothing more.
    fn end(&mut self, projectile: &Projectile, hit: Hit) {
        if let Payload::Action {
            action,
            rank,
            start,
            ..
        } = projectile.payload()
        {
            self.deliveries.deliver(Delivered {
                by: Delivering {
                    source: projectile.source(),
                    action,
                    rank,
                    start,
                    launch: None,
                },
                reach: Reached::End,
                hit,
            });
        }
    }
}
