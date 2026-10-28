use core::ops::ControlFlow;
use std::cmp::Ordering;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res};
use campfire_actions::{
    ActionBook, ActionProgress, ActionRange, ActionSlots, AttackTarget, ProgressIndexes, RangeFit,
    Targets,
};
use campfire_declarations::ById;
use campfire_geometry::Placed;
use campfire_math::Num;
use campfire_sim::{EntityIndex, Position, SimTick, StableId};
use campfire_units::{Body, DamageHits, Dead, Destination, UnitsNear};
use campfire_values::{Relation, TagSet, Team, UnitTags, UnitType};

use crate::attack_move::AttackMove;
use crate::retaliate::Retaliate;
use crate::retaliation_book::RetaliationBook;

/// The calls of damaged units to their allies to answer the units that struck them
/// (04-capabilities/control.md, A damaged unit calls its idle allies).
#[derive(Debug)]
pub struct Retaliating;

impl Retaliating {
    /// Calls, on a server, for each hit of the tick's damage in the order dealt, by a living unit
    /// the mode's aggressors' filter passes as the struck unit regards it, within the mode's
    /// distance of it, edge to edge, each living unit that answers within the mode's radius past
    /// the struck unit's body, the struck unit's own team or an ally's, its own included, dead or
    /// not it, that stands idle as a scan's idle and has a weapon that selects the aggressor: each
    /// takes the aggressor as its attack target, chased from where it stands until the mode's
    /// chase passes, as Zero Hour's `ActiveBody::attemptDamage` calls its friends
    /// (`AIUpdateInterface::retaliate`).
    pub(super) fn call(
        (tick, struck, retaliation, retaliates, actions): (
            Res<'_, SimTick>,
            Res<'_, DamageHits>,
            Res<'_, RetaliationBook>,
            Res<'_, ById<UnitType, Retaliate>>,
            Res<'_, ActionBook>,
        ),
        (targets, near, index): (Targets<'_, '_>, UnitsNear<'_>, Res<'_, EntityIndex>),
        struck_units: Query<'_, '_, (&Position, &Team, Option<&Body>)>,
        mut units: Query<
            '_,
            '_,
            (
                (&Position, &Team, &UnitType),
                (&ActionSlots, &mut ActionProgress),
                (Option<&Destination>, Option<&AttackMove>),
                Option<&UnitTags>,
            ),
            Without<Dead>,
        >,
        (mut indexes, mut called): (ProgressIndexes<'_>, Local<'_, Vec<(StableId, Entity)>>),
    ) {
        let Some(rule) = retaliation.0 else {
            return;
        };
        let metric = targets.metric();
        let until = tick.start().after(rule.chase);
        for hit in &*struck {
            let Some((&position, &team, body)) = index
                .get(hit.target)
                .and_then(|at| struck_units.get(at).ok())
            else {
                continue;
            };
            let Some(aggressor) = targets.living(hit.source) else {
                continue;
            };
            let from = Placed::new(position.get(), Body::shape_of(body));
            let to = Placed::new(aggressor.position, aggressor.shape);
            let relation = targets.relation(team, aggressor.team);
            let reached = targets.reach(from, ActionRange::meters(rule.distance), None, to);
            if !rule.aggressors.selects(relation, aggressor.tags) || reached != RangeFit::Within {
                continue;
            }
            called.clear();
            let bound = rule
                .radius
                .checked_add(from.shape.bound())
                .unwrap_or(Num::MAX);
            near.current().visit_near(position.get(), bound, |entry| {
                let Ok(((at, &ally, &unit_type), (slots, progress), (walk, attack_move), tags)) =
                    units.get(entry.key)
                else {
                    return ControlFlow::Continue(());
                };
                let friend = targets.relation(team, ally) == Relation::Friendly;
                let within = metric.order(from, rule.radius, Placed::point(at.get()));
                let moves = attack_move.is_some_and(|attack_move| attack_move.goal().is_some());
                let walks = walk.is_some_and(|walk| walk.get().is_some());
                let idle = progress.in_progress().is_none()
                    && progress.attack_target().is_none()
                    && (moves || !walks);
                let holder = tags.map_or(TagSet::NONE, |tags| tags.tags);
                let selected = (targets.relation(ally, aggressor.team), aggressor.tags);
                let armed = slots.weapon_in(&actions, holder, Some(selected)).is_some();
                if friend
                    && within != Ordering::Greater
                    && idle
                    && armed
                    && retaliates.get(unit_type).is_some()
                    && targets.living(entry.id).is_some()
                {
                    called.push((entry.id, entry.key));
                }
                ControlFlow::Continue(())
            });
            for &(id, entity) in &*called {
                let Ok(((at, ..), (_, mut progress), ..)) = units.get_mut(entity) else {
                    continue;
                };
                let target = AttackTarget::retaliated(aggressor.id, at.get(), until);
                progress.set_attack_target(Some(target), &mut indexes.of(id, entity));
            }
        }
    }
}
