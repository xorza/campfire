use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::lifecycle::RemovedComponents;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::{Position, SimTick, Unpredicted};
use campfire_units::{Body, Dead, Destination};

use crate::body_index::BodyIndex;
use crate::path_walker::PathWalker;
use crate::pathing_grid::PathingGrid;
use crate::progress::Progress;
use crate::route::{Route, RouteAsk};
use crate::route_asks::{Ask, AskingUnits, Member, RouteAsks, RouteSearch};
use crate::route_planner::{RoutePlanner, Walkable};
use crate::segment::Segment;
use crate::walker::Walker;

/// The walkers' routes to their destinations: asked as they change, and planned.
#[derive(Debug)]
pub struct Routing;

impl Routing {
    /// Keeps each walker's route on its destination. A walker with a new destination asks for a
    /// route there, unless its route reaches its goal and the walker may go straight on from the
    /// waypoint before the last to the new one, past every static body and every cell the walls
    /// block it from, as a chaser after a target that moved: then only the last waypoint moves.
    /// A walker with no destination forgets its route, unless it arrived short. A change of the
    /// static bodies asks the routes it changes as it is tracked, as `StaticTracking` says. A
    /// walker that asks or forgets its route forgets its progress. One on its path asks as one
    /// of its spawn group's party, to its goal. A walker's route follows from its destination and
    /// its route alone: so a run visits the walkers whose destination or route changed since its
    /// last, and those that lived again or that a predicting client predicts again, `returned`,
    /// a buffer it keeps, whose parts kept their ticks while hidden.
    pub(super) fn route_units(
        tick: Res<'_, SimTick>,
        statics: Res<'_, BodyIndex>,
        grid: Option<Res<'_, PathingGrid>>,
        mut units: Query<
            '_,
            '_,
            (
                Entity,
                &Position,
                Ref<'_, Destination>,
                &mut Route,
                &mut Progress,
                Option<&Body>,
                Option<&PathWalker>,
            ),
            Without<Dead>,
        >,
        (mut lived, mut predicted, mut returned): (
            RemovedComponents<'_, '_, Dead>,
            RemovedComponents<'_, '_, Unpredicted>,
            Local<'_, Vec<Entity>>,
        ),
    ) {
        let now = tick.start();
        returned.clear();
        returned.extend(lived.read().chain(predicted.read()));
        returned.sort_unstable();
        for (entity, &at, destination, mut route, mut progress, body, path) in &mut units {
            let changed = destination.is_changed() || route.is_changed();
            if !(changed || returned.binary_search(&entity).is_ok()) {
                continue;
            }
            let walker = Walker::walking(body);
            match destination.get() {
                None if route.arrived_short() => {}
                None => {
                    if route.goal().is_some() {
                        route.clear();
                        progress.restart();
                    }
                }
                Some(goal) if route.goal() == Some(goal) => {}
                Some(goal) => {
                    let from = match route.ahead() {
                        [.., before, _] => Some(*before),
                        [_] => Some(at.get()),
                        [] => None,
                    };
                    let straight = from.filter(|_| route.reached() && route.asked().is_none());
                    let blocks = |from| {
                        grid.as_ref().is_some_and(|grid| {
                            let walkable = Walkable::of(grid.clearance(walker), &statics);
                            walkable.blocks(Segment::new(from, goal))
                        })
                    };
                    if let Some(from) = straight
                        && !blocks(from)
                    {
                        route.move_goal(goal);
                    } else {
                        let ask = RouteAsk {
                            goal,
                            tick: now,
                            party: path.and_then(|path| path.party(goal)),
                        };
                        route.ask(ask, &mut progress);
                    }
                }
            }
        }
    }

    /// Plans the asked routes as the tick's work begins, as `RouteAsks::plan` says, and gives
    /// each unit the route planned for it. Every tick reads the asks into `asks`, buffers it
    /// keeps.
    pub(super) fn plan_routes(
        grid: Option<Res<'_, PathingGrid>>,
        statics: Res<'_, BodyIndex>,
        mut planner: Option<ResMut<'_, RoutePlanner>>,
        mut units: AskingUnits<'_, '_>,
        mut asks: Local<'_, RouteAsks>,
    ) {
        asks.read(units.iter().filter_map(|(entity, &id, &at, route, body)| {
            Some(Ask {
                tick: route.asked()?,
                party: route.party(),
                member: Member {
                    id,
                    entity,
                    pos: at.get(),
                    walker: Walker::walking(body),
                    goal: route.goal().expect("a unit that asked a route has a goal"),
                },
            })
        }));
        let search = grid
            .as_deref()
            .zip(planner.as_deref_mut())
            .map(|(grid, planner)| RouteSearch {
                grid,
                statics: &statics,
                planner,
            });
        asks.plan(search, &mut |member, waypoints, reached| {
            let units = units.get_mut(member.entity);
            let (.., mut route, _) = units.expect("a unit read this tick");
            route.answer(member.pos, waypoints, reached);
        });
    }
}
