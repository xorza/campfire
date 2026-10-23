use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::ActionProgress;
use campfire_declarations::RoleForm;
use campfire_sim::{Position, StableId};
use campfire_units::{Body, Dead, Walk};

use crate::navigation_book::NavigationBook;
use crate::on_path::OnPath;
use crate::path_arrivals::{PathArrival, PathArrivals};
use crate::path_walker::PathWalker;

/// The walk of each path walker along its path.
#[derive(Debug)]
pub struct PathFollowing;

impl PathFollowing {
    /// Sends each path walker with no attack target and no cast walking in range, on its path, to
    /// the waypoint it walks to, and on to the next once the waypoint is within its body, or it
    /// stands on the waypoint with no body: walkers that push each other never stand on one point.
    /// Past the last waypoint it goes as its path's walk says, and, on the server, joins the path
    /// arrivals, by stable id in each tick, whose `on_path_end` the mode runs; a predicting
    /// client's form `F` walks alike and leaves the hook to the server. A walker that chased a
    /// target walks back to where it left its path.
    pub(crate) fn follow_paths<F: RoleForm>(
        navigation: Res<'_, NavigationBook>,
        mut walkers: Query<
            '_,
            '_,
            (
                (&StableId, &Position),
                &OnPath,
                &mut PathWalker,
                Option<&ActionProgress>,
                Walk,
                Option<&Body>,
            ),
            Without<Dead>,
        >,
        (arrivals, mut reached): (
            Option<ResMut<'_, PathArrivals>>,
            Local<'_, Vec<PathArrival>>,
        ),
    ) {
        // A predicting client holds no state of the server's alone.
        debug_assert!(
            F::PREDICTS || arrivals.is_some(),
            "a server holds its arrivals"
        );
        reached.clear();
        let paths = &navigation.paths;
        for ((&id, &position), path, mut walker, progress, mut walk, body) in &mut walkers {
            let busy = |progress: &ActionProgress| {
                progress.attack_target().is_some() || progress.is_approaching()
            };
            if walker.is_left() || progress.is_some_and(busy) {
                continue;
            }
            let path = path.get();
            let mut waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
            if waypoint.is_some_and(|at| {
                position
                    .get()
                    .within_ground(at, Body::shape_of(body).bound())
            }) {
                let len = u32::try_from(paths.points(path).len()).expect("waypoints fit u32");
                let ended = walker.advance(len, paths.walk(path));
                if ended && !F::PREDICTS {
                    reached.push(PathArrival { id, path });
                }
                waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
            }
            walk.to(waypoint);
        }
        if let Some(mut arrivals) = arrivals.filter(|_| !reached.is_empty()) {
            reached.sort_unstable_by_key(|arrival| arrival.id);
            arrivals.0.extend(reached.iter().copied());
        }
    }
}
