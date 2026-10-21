use core::ops::ControlFlow;

use bevy_ecs::query::Without;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_actions::{
    ActionBook, ActionKind, ActionProgress, ActionRange, ActionSlots, AttackTarget,
    ProgressIndexes, Targets,
};
use campfire_declarations::ByType;
use campfire_geometry::Placed;
use campfire_math::Num;
use campfire_sim::{DueIndex, DueUnit, Position, SimTick, StableId};
use campfire_units::{Body, Dead, Destination, UnitsNear};
use campfire_values::{Team, UnitType};
use campfire_vision::{ByVision, SeenTargets};

use crate::acquire::Acquire;
use crate::next_scan::NextScan;

/// The idle scans of the units of types that declare one (04-capabilities/control.md, An idle
/// unit scans for a target, as its type declares).
#[derive(Debug)]
pub struct Acquiring;

/// A unit a scan may take: how far its center lies, squared, in bits, and its stable id, which
/// orders two at one distance.
type Candidate = (u128, StableId);

impl Acquiring {
    /// Scans for each unit due this tick that stands idle, with no destination, nothing under way
    /// and no attack target: it takes as its attack target, from its scan, the nearest unit its
    /// team sees, center to center, that a filter of its type's targets selects and that the
    /// first of its weapons that selects it reaches, the lower stable id at one distance; then it
    /// is due a period later, idle or not (04-capabilities/control.md, The scan is the
    /// server's).
    pub(super) fn scan(
        (tick, actions, book): (
            Res<'_, SimTick>,
            Res<'_, ActionBook>,
            Res<'_, ByType<Acquire>>,
        ),
        (targets, seen, near): (
            Targets<'_, '_>,
            SeenTargets<'_, '_, ByVision>,
            UnitsNear<'_>,
        ),
        mut units: Query<
            '_,
            '_,
            (
                (&Position, &Team, &UnitType, &NextScan),
                (&ActionSlots, &mut ActionProgress),
                Option<&Destination>,
                Option<&Body>,
            ),
            Without<Dead>,
        >,
        (mut index, mut taken, mut indexes): (
            ResMut<'_, DueIndex<NextScan>>,
            Local<'_, Vec<DueUnit>>,
            ProgressIndexes<'_>,
        ),
        mut commands: Commands<'_, '_>,
    ) {
        let now = tick.start();
        taken.clear();
        index.take_due(now, &mut taken);
        for &DueUnit { id, entity } in &*taken {
            let Ok((
                (&position, &team, &unit_type, &next),
                (slots, mut progress),
                destination,
                body,
            )) = units.get_mut(entity)
            else {
                continue;
            };
            let Some(acquire) = book.get(unit_type) else {
                continue;
            };
            let due = next.due_at(id, acquire.period, now);
            if due > now {
                index.push(due, id, entity);
                continue;
            }
            commands
                .entity(entity)
                .insert(NextScan::new(now.after(acquire.period)));
            let idle = progress.in_progress().is_none()
                && progress.attack_target().is_none()
                && destination.is_none_or(|destination| destination.get().is_none());
            if !idle {
                continue;
            }
            let from = Placed::new(position.get(), Body::shape_of(body));
            let Some(reach) = Self::reach(slots, &actions) else {
                continue;
            };
            let metric = targets.metric();
            let mut nearest: Option<Candidate> = None;
            let bound = reach.checked_add(from.shape.bound()).unwrap_or(Num::MAX);
            near.current().visit_near(position.get(), bound, |entry| {
                let other = entry.id;
                if other == id {
                    return ControlFlow::Continue(());
                }
                let Some(unit) = targets.enemy(team, other) else {
                    return ControlFlow::Continue(());
                };
                let relation = targets.relation(team, unit.team);
                let weapon = slots.weapon_in(&actions, Some((relation, unit.tags)));
                let range = weapon.map(|slot| slots.range_in(&actions, slot));
                let reached = match range {
                    Some(ActionRange::Meters(range)) => targets.reaches(from, range.get(), &unit),
                    Some(ActionRange::Global) => true,
                    None => false,
                };
                if reached && acquire.targets.selects(relation, unit.tags) && seen.sees(team, other)
                {
                    let distance = metric
                        .offset(position.get(), unit.position)
                        .length_squared_bits();
                    let candidate = (distance, other);
                    if nearest.is_none_or(|best| candidate < best) {
                        nearest = Some(candidate);
                    }
                }
                ControlFlow::Continue(())
            });
            if let Some((_, target)) = nearest {
                let target = Some(AttackTarget::scanned(target));
                progress.set_attack_target(target, &mut indexes.of(id, entity));
            }
        }
    }

    /// The widest range of the learned weapons of `slots`, of `actions`: how far a scan looks past
    /// the unit's edge, every distance for a weapon of the whole map; none for a unit with no
    /// learned weapon.
    fn reach(slots: &ActionSlots, actions: &ActionBook) -> Option<Num> {
        let weapons = slots
            .indexed()
            .filter(|&(at, _)| slots.kind_in(actions, at) == Some(ActionKind::Attack));
        let ranges = weapons.filter_map(|(_, slot)| match slot.values_in(actions)?.range {
            ActionRange::Meters(range) => Some(range.get()),
            ActionRange::Global => Some(Num::MAX),
        });
        ranges.max()
    }
}
