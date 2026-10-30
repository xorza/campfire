use std::num::NonZeroI64;
use std::ops::ControlFlow;

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Query, Res};
use campfire_declarations::ById;
use campfire_geometry::{Bounds, Placed, Radius, Shape};
use campfire_math::{Num, SinCos, Vec3};
use campfire_sim::{MatchRate, Position};
use campfire_units::{
    Body, Dead, Destination, FlightGoal, Layer, MapBook, OnWings, StatusTags, UnitsNear, Walk,
};
use campfire_values::UnitType;

use crate::Navigation;
use crate::altitude::Altitude;
use crate::arrived_at::ArrivedAt;
use crate::body_index::BodyIndex;
use crate::flight_data::FlightData;
use crate::landing_book::LandingBook;
use crate::pathing_grid::PathingGrid;
use crate::route_planner::{RoutePlanner, Walkable};
use crate::segment::Segment;
use crate::walker::Walker;

/// Navigation's fliers: units of a type of `flight`, whose height it alone writes.
#[derive(Debug)]
pub(crate) struct Fliers;

impl Fliers {
    /// Flies each living flier, after every move of the tick: its height in the world moves toward
    /// the ground under it plus its type's height, or toward the ground for one its `FlightGoal`
    /// holds there or lands while it walks nowhere, by its climb a tick, or toward the height its
    /// goal names by the goal's climb, never below the ground, and its position takes the height
    /// above the ground that gives; it holds the engine tag `air_target` while that height passes
    /// its type's airborne height, and a winged flier `on_wings` too
    /// (04-capabilities/navigation.md, A flier holds a height; 04-capabilities/stats.md, Airborne
    /// is a tag of height; 04-capabilities/navigation.md, A jet flies on wings).
    pub(crate) fn fly(
        (map, rate, book): (
            Res<'_, MapBook>,
            Res<'_, MatchRate>,
            Res<'_, ById<UnitType, FlightData>>,
        ),
        mut fliers: Query<
            '_,
            '_,
            (
                &UnitType,
                &mut Position,
                &mut Altitude,
                &mut StatusTags,
                &FlightGoal,
                Option<&Destination>,
            ),
            Without<Dead>,
        >,
    ) {
        let hz = NonZeroI64::from(rate.hz()).get();
        for (&unit_type, mut position, mut altitude, mut status, &goal, destination) in &mut fliers
        {
            let walks = destination.is_some_and(|destination| destination.get().is_some());
            let flight = *book.get(unit_type).expect("a flier's type has its flight");
            let at = position.get();
            let ground = map.point(Vec3::new(at.x, Num::ZERO, at.z)).y;
            let (goal, climb) = match goal {
                FlightGoal::Height => (ground + flight.height, flight.climb / hz),
                FlightGoal::Land | FlightGoal::LandAt(_) if walks => {
                    (ground + flight.height, flight.climb / hz)
                }
                FlightGoal::Ground | FlightGoal::Land | FlightGoal::LandAt(_) => {
                    (ground, flight.climb / hz)
                }
                FlightGoal::Toward { height, climb } => (ground + height, climb),
            };
            let now = altitude.get();
            let next = if now < goal {
                (now + climb).min(goal)
            } else {
                (now - climb).max(goal)
            };
            let next = next.max(ground);
            altitude.set_if_neq(Altitude::new(next));
            let height = next - ground;
            position.set_if_neq(Position::from(Vec3::new(at.x, height, at.z)));
            let airborne = height > flight.airborne;
            let target = status
                .turned(Navigation::AIR_TARGET, airborne)
                .turned(OnWings::TAG, airborne && flight.wings.is_some());
            status.set_if_neq(target);
        }
    }

    /// Finds, before the tick's flight, the point each living flier lands at that lands, walks
    /// nowhere and is still in the air, and holds it in its goal, so it lands there whatever walks
    /// under it after, as Zero Hour's transport takes its point once, as its landing begins
    /// (riders-and-upgrades.md, A flier lands where Zero Hour's transport lands). It finds a point
    /// again only once it hovers away from it, at no arrival there. A flier lands where it hovers
    /// when its body has room on the ground there; else at the first point of rings about it,
    /// outward by the mode's ring, at which a circle of the mode's probe clears the walls, the
    /// static bodies and the first layer's units, as `findPositionAround` takes it, moved to the
    /// first cell of the mode's spiral about that point a walker of its radius on the first layer
    /// may stand in, clear of the bodies of the first layer's units, as
    /// `adjustToLandingDestination` walks it, or kept when no such cell is in the spiral; it walks
    /// there first. One whose rings find no point, or of a radius no kind of walker has, lands
    /// where it hovers.
    pub(crate) fn find_ground(
        (grid, planner, statics, landing): (
            Option<Res<'_, PathingGrid>>,
            Option<Res<'_, RoutePlanner>>,
            Res<'_, BodyIndex>,
            Res<'_, LandingBook>,
        ),
        units: UnitsNear<'_>,
        mut fliers: Query<
            '_,
            '_,
            (
                Entity,
                &Position,
                Option<&Body>,
                &mut FlightGoal,
                Option<&ArrivedAt>,
                Walk,
            ),
            Without<Dead>,
        >,
        bodies: Query<'_, '_, &Body, Without<Dead>>,
    ) {
        let (Some(grid), Some(planner)) = (grid, planner) else {
            return;
        };
        let LandingBook(landing) = *landing;
        for (entity, position, body, mut goal, arrived, mut walk) in &mut fliers {
            let at = position.get();
            let found = match *goal {
                FlightGoal::Land => None,
                FlightGoal::LandAt(point) => Some(point),
                FlightGoal::Height | FlightGoal::Ground | FlightGoal::Toward { .. } => continue,
            };
            if walk.goal().is_some() || at.y <= Num::ZERO {
                continue;
            }
            let ground = Vec3::new(at.x, Num::ZERO, at.z);
            let arrived = arrived.and_then(|arrived| arrived.get());
            if found.is_some_and(|point| point == ground || arrived == Some(point)) {
                continue;
            }
            let point = 'found: {
                let radius = body.map_or(Some(Num::ZERO), |body| body.radius());
                let walker = radius.map(|radius| Walker {
                    layer: Layer::FIRST,
                    radius,
                });
                let Some((radius, clearance)) =
                    radius.zip(walker.and_then(|walker| grid.serving(walker)))
                else {
                    break 'found ground;
                };
                let clear_of_units = |at: Vec3, shape: Shape| {
                    let landed = Placed::new(at, shape);
                    let mut clear = true;
                    units.current().visit_near(at, shape.bound(), |unit| {
                        let under = unit.key != entity
                            && bodies.get(unit.key).is_ok_and(|other| {
                                other.layer() == Layer::FIRST && landed.overlaps(unit.body)
                            });
                        if under {
                            clear = false;
                            return ControlFlow::Break(());
                        }
                        ControlFlow::Continue(())
                    });
                    clear
                };
                let shape = Body::shape_of(body);
                let walkable = Walkable::of(clearance, &statics);
                if !walkable.blocks(Segment::new(ground, ground)) && clear_of_units(ground, shape) {
                    break 'found ground;
                }
                let cells = clearance.grid();
                let probe = landing.probe.map_or(radius, Radius::get);
                let probed = Walker {
                    layer: Layer::FIRST,
                    radius: probe,
                };
                let circle = Radius::new(probe).map_or(Shape::POINT, Shape::Circle);
                let probe_clears = |point: Vec3| {
                    cells.cell_of(point).is_some()
                        && !clearance.walled_at(point)
                        && !statics.blocks(Segment::new(point, point), probed)
                        && clear_of_units(point, circle)
                };
                let ring = landing.ring.map_or_else(|| cells.cell(), Radius::get);
                let Some(first) = Self::first_clear(ground, ring, cells.bounds(), probe_clears)
                else {
                    break 'found ground;
                };
                let spiral = landing.cells.map_or_else(
                    || cells.cells(),
                    |cells| usize::try_from(cells.get()).expect("a spiral's cells fit a usize"),
                );
                let mut work = 0;
                let cell =
                    planner.spiral_open_where(walkable, first, (spiral, &mut work), |cell| {
                        clear_of_units(cells.center(cell, Num::ZERO), shape)
                    });
                planner.count(work);
                cell.map_or(first, |cell| cells.center(cell, Num::ZERO))
            };
            if point != ground {
                walk.to(Some(point));
            }
            goal.set_if_neq(FlightGoal::LandAt(point));
        }
    }

    /// The first point of rings about `center` that `clears` takes: `center`, then rings `ring`
    /// apart outward, each sampled at angles of `ring` × 60° over the ring's radius plus 1 apart,
    /// from 0° each way in turn, as `PartitionManager::findPositionAround` samples them, until a
    /// ring's radius passes the corner of `bounds` farthest from `center`, past which no point lies
    /// in them; none when no point of them clears.
    fn first_clear(
        center: Vec3,
        ring: Num,
        bounds: &Bounds,
        clears: impl Fn(Vec3) -> bool,
    ) -> Option<Vec3> {
        if clears(center) {
            return Some(center);
        }
        let ([low_x, low_z], [high_x, high_z]) = (bounds.min(), bounds.max());
        let reach = |low: Num, high: Num, at: Num| (at - low).max(high - at);
        let [x, z] = [
            reach(low_x, high_x, center.x),
            reach(low_z, high_z, center.z),
        ];
        let farthest = x * x + z * z;
        let mut distance = ring;
        while distance * distance <= farthest {
            let spacing = ring * Num::int(60) / (distance + Num::ONE);
            let samples = (Num::int(180) / spacing).ceil();
            let at = |degrees: Num| {
                let SinCos { sin, cos } = SinCos::of_degrees(degrees);
                Vec3::new(
                    center.x + distance * cos,
                    Num::ZERO,
                    center.z - distance * sin,
                )
            };
            for sample in 0..samples {
                let turn = spacing * Num::int(sample);
                if clears(at(turn)) {
                    return Some(at(turn));
                }
                if sample != 0 && clears(at(-turn)) {
                    return Some(at(-turn));
                }
            }
            distance += ring;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    #[test]
    fn rings_sample_each_way_in_turn_until_they_pass_the_bounds() {
        // Rings 1 m apart about the origin in bounds of 4 m a side, whose corners lie √8 m off:
        // rings of 1 and 2 m, as 3 m passes them, sampled 60° / 2 and / 3 apart, 30° and 20°, so
        // ⌈180 / 30⌉ = 6 and 9 samples, each but 0° tried both ways: 11 and 17 points after the
        // center, 29 in all, none clear.
        let bounds = Bounds::new([-Num::int(2); 2], [Num::int(2); 2]).unwrap();
        let tried = RefCell::new(Vec::new());
        let none = Fliers::first_clear(Vec3::ZERO, Num::ONE, &bounds, |point| {
            tried.borrow_mut().push(point);
            false
        });
        assert_eq!(none, None);
        let tried = tried.into_inner();
        assert_eq!(tried.len(), 29);
        assert_eq!(
            tried[..2],
            [Vec3::ZERO, Vec3::new(Num::ONE, Num::ZERO, Num::ZERO)]
        );
        // Each angle but 0° is tried turned one way, then the other, at one x, so the z of each
        // pair are opposite, the first below 0; the x falls as the angle grows.
        let first_ring = &tried[2..12];
        for pair in first_ring.chunks(2) {
            assert_eq!((pair[0].x, pair[0].z), (pair[1].x, -pair[1].z));
            assert!(pair[0].z < Num::ZERO);
        }
        assert!(first_ring.chunks(2).is_sorted_by(|a, b| a[0].x > b[0].x));
        // The second ring starts at 0°, 2 m off.
        assert_eq!(tried[12], Vec3::new(Num::int(2), Num::ZERO, Num::ZERO));

        // A center that clears is the point, the one tried; a test that takes only points of a
        // positive z takes the first one turned the other way, −30° on the ring of 1 m, the fourth
        // tried.
        let center = Vec3::new(Num::int(3), Num::ZERO, Num::int(4));
        let calls = RefCell::new(0);
        let found = Fliers::first_clear(center, Num::ONE, &bounds, |_| {
            *calls.borrow_mut() += 1;
            true
        });
        assert_eq!((found, calls.take()), (Some(center), 1));
        let found = Fliers::first_clear(Vec3::ZERO, Num::ONE, &bounds, |point| point.z > Num::ZERO);
        assert_eq!(found, Some(tried[3]));
    }
}
