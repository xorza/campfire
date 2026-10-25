use core::ops::ControlFlow;

use bevy_ecs::query::Without;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_actions::{
    ActionBook, ActionKind, ActionProgress, ActionRange, ActionSlots, AttackTarget,
    ProgressIndexes, RangeFit, Targets,
};
use campfire_declarations::ById;
use campfire_geometry::Placed;
use campfire_math::Num;
use campfire_sim::{DueIndex, DueUnit, Position, SimTick, StableId};
use campfire_units::{Body, Dead, UnitsNear, Walk};
use campfire_values::{TagSet, Team, UnitTags, UnitType};
use campfire_vision::{ByVision, SeenTargets};

use crate::acquire::{Acquire, NextScan};
use crate::attack_move::AttackMove;

/// The idle scans of the units of types that declare one (04-capabilities/control.md, An idle
/// unit scans for a target, as its type declares).
#[derive(Debug)]
pub struct Acquiring;

/// A unit a scan may take: how far its center lies, squared, in bits, and its stable id, which
/// orders two at one distance.
type Candidate = (u128, StableId);

impl Acquiring {
    /// Scans for each unit due this tick that stands idle, with no destination, nothing under way
    /// and no attack target, or that attack-moves with nothing under way and no attack target: it
    /// takes as its attack target, from its scan, the nearest unit its team sees, center to
    /// center, that a filter of its type's targets selects and that the first of its weapons that
    /// selects it reaches, the lower stable id at one distance, and one that attack-moves stops
    /// its walk to attack it; then it is due a period later, idle or not
    /// (04-capabilities/control.md, The scan is the server's; 04-capabilities/control.md, An army
    /// attack-moves).
    pub(super) fn scan(
        (tick, actions, book): (
            Res<'_, SimTick>,
            Res<'_, ActionBook>,
            Res<'_, ById<UnitType, Acquire>>,
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
                (Option<Walk>, Option<&mut AttackMove>),
                (Option<&Body>, Option<&UnitTags>),
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
                (walk, attack_move),
                (body, tags),
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
            let moves = attack_move
                .as_ref()
                .is_some_and(|attack_move| attack_move.goal().is_some());
            let walks = walk.as_ref().is_some_and(|walk| walk.goal().is_some());
            let idle = progress.in_progress().is_none()
                && progress.attack_target().is_none()
                && (moves || !walks);
            if !idle {
                continue;
            }
            let holder = tags.map_or(TagSet::NONE, |tags| tags.tags);
            let from = Placed::new(position.get(), Body::shape_of(body));
            let Some(reach) = Self::reach(slots, &actions, holder) else {
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
                let weapon = slots.weapon_in(&actions, holder, Some((relation, unit.tags)));
                let to = Placed::new(unit.position, unit.shape);
                let reached = weapon.is_some_and(|slot| {
                    let values = slots
                        .values_in(&actions, slot)
                        .expect("a weapon's slot is learned");
                    targets.reach(from, values.range, values.min_range, to) == RangeFit::Within
                });
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
                if let (Some(mut walk), Some(mut attack_move)) = (walk, attack_move)
                    && walks
                    && moves
                {
                    walk.to(None);
                    attack_move.stop();
                }
            }
        }
    }

    /// The widest range of the learned weapons of `slots`, of `actions`, whose `when` the unit's
    /// tags, `holder`, pass: how far a scan looks past the unit's edge, every distance for a
    /// weapon of the whole map; none for a unit with no such weapon.
    fn reach(slots: &ActionSlots, actions: &ActionBook, holder: TagSet) -> Option<Num> {
        let weapons = slots.indexed().filter(|&(at, slot)| {
            let usable = slot.action.and_then(|action| actions.get(action));
            slots.kind_in(actions, at) == Some(ActionKind::Attack)
                && usable.is_some_and(|action| action.usable_by(holder))
        });
        let ranges = weapons.filter_map(|(_, slot)| match slot.values_in(actions)?.range {
            ActionRange::Meters(range) => Some(range.get()),
            ActionRange::Global => Some(Num::MAX),
        });
        ranges.max()
    }
}
