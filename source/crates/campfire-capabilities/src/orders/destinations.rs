use bevy_ecs::query::Without;
use bevy_ecs::system::{Query, Res};
use campfire_sim::Position;

use crate::actions::action_book::ActionBook;
use crate::actions::action_range::ActionRange;
use crate::actions::action_slots::ActionSlots;
use crate::actions::targets::Targets;
use crate::navigation::destination::Destination;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::paths::Paths;
use crate::navigation::route::Route;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::team::Team;

/// The destinations units walk to on their own: along their paths, and after their attack targets.
#[derive(Debug)]
pub(super) struct Destinations;

impl Destinations {
    /// Sends each path walker with no attack target and no cast walking in range, on its path, to
    /// the waypoint it walks to, and on to the next once the waypoint is within its body, or it
    /// stands on the waypoint with no body: walkers that push each other never stand on one point.
    /// A walker that chased a target walks back to where it left its path.
    pub(super) fn follow_paths(
        paths: Res<'_, Paths>,
        mut walkers: Query<
            '_,
            '_,
            (
                &Position,
                &OnPath,
                &mut PathWalker,
                Option<&ActionSlots>,
                &mut Destination,
                Option<&Route>,
                Option<&Body>,
            ),
            Without<Dead>,
        >,
    ) {
        for (&position, path, mut walker, slots, mut destination, route, body) in &mut walkers {
            let busy = |slots: &ActionSlots| slots.attack_target().is_some() || slots.approaching();
            if walker.left() || slots.is_some_and(busy) {
                continue;
            }
            let path = path.get();
            let mut waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
            if waypoint.is_some_and(|at| position.within_ground(at, Body::shape_of(body).bound())) {
                walker.advance();
                waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
            }
            Destination::walk_to(&mut destination, route, waypoint);
        }
    }

    /// Walks each unit that can move to its attack target while out of the range of the weapon it
    /// attacks it with, and stops it in range or in its windup; a cast that walks in range walks
    /// the unit instead. A unit whose target is gone, dead, no longer an enemy or one no weapon of
    /// it selects drops it and stops.
    pub(super) fn chase(
        book: Res<'_, ActionBook>,
        targets: Targets<'_, '_>,
        mut chasers: Query<
            '_,
            '_,
            (
                &Position,
                &Team,
                &mut ActionSlots,
                &mut Destination,
                Option<&Route>,
                Option<&Body>,
            ),
            Without<Dead>,
        >,
    ) {
        for (&position, &team, mut slots, mut destination, route, body) in &mut chasers {
            let Some(target) = slots.attack_target() else {
                continue;
            };
            if slots.attacking().is_some() || slots.approaching() {
                continue;
            }
            let aimed = targets.enemy(team, target).and_then(|unit| {
                let selected = (targets.relation(team, unit.team), unit.tags);
                let slot = book.weapon_for(&slots, Some(selected))?;
                Some((unit, book.range(&slots, slot)))
            });
            match aimed {
                None => {
                    slots.set_attack_target(None);
                    Destination::walk_to(&mut destination, route, None);
                }
                Some((unit, ActionRange::Meters(range)))
                    if !targets.reaches(position, Body::shape_of(body), range, &unit) =>
                {
                    Destination::walk_to(&mut destination, route, Some(unit.pos));
                }
                Some(_) => Destination::walk_to(&mut destination, route, None),
            }
        }
    }
}
