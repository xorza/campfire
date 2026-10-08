use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::{Position, SimTick};

use crate::navigation::body_index::BodyIndex;
use crate::navigation::destination::Destination;
use crate::navigation::party::{Party, PartyKey};
use crate::navigation::path_walker::PathWalker;
use crate::navigation::pathing_grid::PathingGrid;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::navigation::route_asks::{AskingUnits, RouteAsks};
use crate::navigation::route_planner::{RoutePlanner, Walkable};
use crate::navigation::segment::Segment;
use crate::navigation::static_changes::StaticChanges;
use crate::navigation::walker::Walker;
use crate::units::body::Body;
use crate::units::dead::Dead;

/// The walkers' routes to their destinations: asked as they change, and planned.
#[derive(Debug)]
pub(super) struct Routing;

impl Routing {
    /// Keeps each walker's route on its destination. A walker with a new destination asks for a
    /// route there, unless its route reaches its goal and the walker may go straight on from the
    /// waypoint before the last to the new one, past every static body and every cell the walls
    /// block it from, as a chaser after a target that moved: then only the last waypoint moves.
    /// After the static bodies changed, a walker whose way along its route a static body of its
    /// layer blocks asks for its route again; after they lost a body, so does one whose route ends
    /// short of its goal, and one that arrived short of it walks there again. A walker with no
    /// destination forgets its route, unless it arrived short. With no pathing grid, as in a match
    /// with no map, no static body blocks a route. Each walker checks its route only when the
    /// static bodies changed since the tick before, against the bodies put in, which `changes`
    /// holds; as a route the static bodies do not block stays clear until one is put in, the
    /// outcome is that of checking every tick. A walker that asks or forgets its route forgets its
    /// progress. One on its path asks as one of its spawn group's party, to its goal.
    pub(super) fn route_units(
        tick: Res<'_, SimTick>,
        (statics, mut changes): (Res<'_, BodyIndex>, ResMut<'_, StaticChanges>),
        grid: Option<Res<'_, PathingGrid>>,
        mut units: Query<
            '_,
            '_,
            (
                &Position,
                &mut Destination,
                &mut Route,
                &mut Progress,
                Option<&Body>,
                Option<&PathWalker>,
            ),
            Without<Dead>,
        >,
    ) {
        let now = tick.start();
        let planned = grid.is_some();
        let opened = planned && changes.removed();
        for (&at, mut destination, mut route, mut progress, body, path) in &mut units {
            let walker = Walker::walking(body);
            let party = |goal| {
                let group = path.filter(|path| !path.left())?.group();
                Some(Party {
                    key: PartyKey::Spawn(group),
                    goal,
                })
            };
            match destination.get() {
                None if route.arrived_short() => {
                    if opened {
                        let goal = route.goal().expect("a route that arrived short has a goal");
                        destination.set_if_neq(Destination::to(Some(goal)));
                        route.ask(goal, now, party(goal));
                        progress.restart();
                    }
                }
                None => {
                    if route.goal().is_some() {
                        route.clear();
                        progress.restart();
                    }
                }
                Some(goal) if route.goal() == Some(goal) => {
                    let blocked = planned && changes.blocks_route(at, route.ahead(), walker);
                    let unreached = opened && !route.reached();
                    if route.asked().is_none() && (blocked || unreached) {
                        route.ask(goal, now, party(goal));
                        progress.restart();
                    }
                }
                Some(goal) => {
                    let from = match route.ahead() {
                        [.., before, _] => Some(*before),
                        [_] => Some(at),
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
                        route.ask(goal, now, party(goal));
                        progress.restart();
                    }
                }
            }
        }
        changes.clear();
    }

    /// Plans the asked routes as the tick's work begins, as `RouteAsks::plan` says. Every tick
    /// reads the asks into `asks`, buffers it keeps.
    pub(super) fn plan_routes(
        grid: Option<Res<'_, PathingGrid>>,
        statics: Res<'_, BodyIndex>,
        mut planner: Option<ResMut<'_, RoutePlanner>>,
        mut units: AskingUnits<'_, '_>,
        mut asks: Local<'_, RouteAsks>,
    ) {
        asks.plan(
            grid.as_deref(),
            &statics,
            planner.as_deref_mut(),
            &mut units,
        );
    }
}
