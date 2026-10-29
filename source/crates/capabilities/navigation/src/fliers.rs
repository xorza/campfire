use std::num::NonZeroI64;
use std::ops::ControlFlow;

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Query, Res};
use campfire_declarations::ById;
use campfire_geometry::Placed;
use campfire_math::{Num, Vec3};
use campfire_sim::{MatchRate, Position};
use campfire_units::{
    Body, Dead, Destination, FlightGoal, Layer, MapBook, OnWings, StatusTags, UnitsNear, Walk,
};
use campfire_values::UnitType;

use crate::Navigation;
use crate::altitude::Altitude;
use crate::body_index::BodyIndex;
use crate::flight_data::FlightData;
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
                FlightGoal::Land if walks => (ground + flight.height, flight.climb / hz),
                FlightGoal::Ground | FlightGoal::Land => (ground, flight.climb / hz),
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

    /// Sends each living flier that lands, walks nowhere and is still in the air, before the tick's
    /// flight, over a point where its body has no room on the ground, to the nearest point where it
    /// has: the center of the nearest cell of the pathing grid a walker of its radius on the first
    /// layer may stand in, clear of the bodies of the first layer's units, as Zero Hour's Chinook
    /// lands at the point `findPositionAround`, which takes no point inside an object, and
    /// `adjustToLandingDestination` find clear (riders-and-upgrades.md, A flier lands where it has
    /// room); one of a radius no kind of walker has lands where it stands.
    pub(crate) fn find_ground(
        (grid, planner, statics): (
            Option<Res<'_, PathingGrid>>,
            Option<Res<'_, RoutePlanner>>,
            Res<'_, BodyIndex>,
        ),
        units: UnitsNear<'_>,
        mut fliers: Query<
            '_,
            '_,
            (Entity, &Position, Option<&Body>, &FlightGoal, Walk),
            Without<Dead>,
        >,
        bodies: Query<'_, '_, &Body, Without<Dead>>,
    ) {
        let (Some(grid), Some(planner)) = (grid, planner) else {
            return;
        };
        for (entity, position, body, &goal, mut walk) in &mut fliers {
            let at = position.get();
            if goal != FlightGoal::Land || walk.goal().is_some() || at.y <= Num::ZERO {
                continue;
            }
            let radius = body.map_or(Some(Num::ZERO), |body| body.radius());
            let walker = radius.map(|radius| Walker {
                layer: Layer::FIRST,
                radius,
            });
            let Some(clearance) = walker.and_then(|walker| grid.serving(walker)) else {
                continue;
            };
            let shape = Body::shape_of(body);
            let clear = |at: Vec3| {
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
            let ground = Vec3::new(at.x, Num::ZERO, at.z);
            let walkable = Walkable::of(clearance, &statics);
            if !walkable.blocks(Segment::new(ground, ground)) && clear(ground) {
                continue;
            }
            let cells = clearance.grid();
            let landing = planner.nearest_open_where(walkable, ground, &mut 0, |cell| {
                clear(cells.center(cell, Num::ZERO))
            });
            if let Some(cell) = landing {
                walk.to(Some(cells.center(cell, Num::ZERO)));
            }
        }
    }
}
