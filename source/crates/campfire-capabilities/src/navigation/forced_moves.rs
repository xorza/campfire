use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, Without};
use bevy_ecs::system::{Commands, Local, ParamSet, Query, Res, ResMut};
use campfire_math::{Num, Vec3};
use campfire_sim::{EntityIndex, Keyed, Ordered, Position, SimTick, StableId, Unpredicted};

use crate::deliveries::Deliveries;
use crate::geometry::bounds::Bounds;
use crate::geometry::shape::Shape;
use crate::navigation::body_index::BodyIndex;
use crate::navigation::destination::Destination;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::segment::Segment;
use crate::navigation::walker::Walker;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::forced_move::{DashTo, ForcedMove, Goal};

/// The forced moves of the units: their dashes, knock backs and teleports.
#[derive(Debug)]
pub(super) struct ForcedMoves;

impl ForcedMoves {
    /// Moves each unit a forced move moves, by stable id, once the units walked, so a dash at a
    /// unit follows its place of this tick, at the unit's own height. A step whose way a static
    /// body of the unit's layer blocks is not taken, nor a knock back's whose way the walls block,
    /// but in the cell it starts in; one past the bounds stops on them; either ends the move. A
    /// dash crosses walls, and one that ends where its walker may not stand goes to the nearest
    /// cell it may, as a teleport does. A unit whose move ended walks its route again from there,
    /// and a dash that delivers an action queues that action's end there, its hooks to run in Hit.
    pub(super) fn force_units(
        (tick, bounds, statics, index): (
            Res<'_, SimTick>,
            Res<'_, Bounds>,
            Res<'_, BodyIndex>,
            Res<'_, EntityIndex>,
        ),
        (grid, planner): (Option<Res<'_, PathingGrid>>, Option<Res<'_, RoutePlanner>>),
        mut deliveries: Option<ResMut<'_, Deliveries>>,
        mut units: ParamSet<
            '_,
            '_,
            (
                Query<'_, '_, (&Position, Option<&Body>), (Without<Dead>, Allow<Unpredicted>)>,
                Query<
                    '_,
                    '_,
                    (
                        Entity,
                        &StableId,
                        &mut Position,
                        &mut ForcedMove,
                        Option<&Body>,
                    ),
                    Without<Dead>,
                >,
            ),
        >,
        mut walkers: Query<'_, '_, (&Destination, &mut Route, &mut Progress)>,
        mut commands: Commands<'_, '_>,
        mut order: Local<'_, Ordered>,
    ) {
        let now = tick.start();
        let ordered = {
            let moving = units.p1();
            order.sort(moving.iter().map(|(entity, &id, ..)| Keyed { id, entity }))
        };
        for &Keyed { entity, .. } in ordered {
            let (at, mut forced, body) = {
                let moving = units.p1();
                let (_, _, &at, &forced, body) = moving.get(entity).expect("a unit in the order");
                (at, forced, body.copied())
            };
            let walker = Walker::walking(body.as_ref());
            let ground = |place: Vec3| Vec3::new(place.x, at.get().y, place.z);
            let goal = match forced {
                ForcedMove::Dash {
                    to: DashTo::Point(point),
                    ..
                } => Some(Goal {
                    at: ground(point.get()),
                    reach: Num::ZERO,
                }),
                ForcedMove::Dash {
                    to: DashTo::Unit(target),
                    ..
                } => {
                    let targets = units.p0();
                    let found = index.get(target).and_then(|unit| targets.get(unit).ok());
                    // A dash at a box makes for its nearest point, as at a point it must touch.
                    found.map(|(&place, body)| match Body::shape_of(body) {
                        Shape::Circle(radius) => Goal {
                            at: ground(place.get()),
                            reach: walker.radius + radius,
                        },
                        Shape::Box(body) => Goal {
                            at: ground(body.nearest_point(place, at).get()),
                            reach: walker.radius,
                        },
                    })
                }
                ForcedMove::KnockBack { to, .. } => Some(Goal {
                    at: to,
                    reach: Num::ZERO,
                }),
            };
            let advanced = forced.advance(at.get(), goal);
            let to = bounds.ground_point([advanced.at.x, advanced.at.z], at);
            let past = to.get() != advanced.at;
            let clearance = grid.as_deref().and_then(|grid| grid.serving(walker));
            let step = Segment::new(at, to);
            let knocked = matches!(forced, ForcedMove::KnockBack { .. });
            let walled =
                knocked && clearance.is_some_and(|clearance| clearance.walls_block_step(step));
            let blocked = statics.blocks(step, walker) || walled;
            let ends = advanced.ends || past || blocked;
            let stands = if blocked { at } else { to };
            let place = match (clearance, planner.as_deref()) {
                (Some(clearance), Some(planner)) if ends && !knocked => {
                    let walkable = Walkable::of(clearance, &statics);
                    planner.stand_at(walkable, stands).unwrap_or(stands)
                }
                _ => stands,
            };
            let mut moving = units.p1();
            let (_, _, mut position, mut under_way, _) =
                moving.get_mut(entity).expect("a unit in the order");
            position.set_if_neq(place);
            if let ForcedMove::Dash {
                to: dash_to,
                delivers: Some(delivery),
                ..
            } = &mut forced
            {
                delivery.went(at, stands);
                if ends {
                    let target = match *dash_to {
                        DashTo::Unit(target) => Some(target),
                        DashTo::Point(_) => None,
                    };
                    let direction = at.ground_offset(stands).normalized();
                    deliveries
                        .as_deref_mut()
                        .expect("a dash that delivers an action runs in a match with abilities")
                        .end_dash(*delivery, target, place, direction);
                }
            }
            if ends {
                commands.entity(entity).remove::<ForcedMove>();
                if let Ok((destination, mut route, mut progress)) = walkers.get_mut(entity) {
                    route.ask_again(destination, &mut progress, now);
                }
            } else {
                under_way.set_if_neq(forced);
            }
        }
    }
}
