use std::cmp::Ordering;

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_actions::{
    ActionBook, ActionProgress, ActionSlots, ProgressIndexes, RangeBonus, RangeFit, Stretch,
    TargetSource, Targets,
};
use campfire_common::Tick;
use campfire_declarations::ById;
use campfire_geometry::Placed;
use campfire_math::{Num, Vec3};
use campfire_sim::{DueIndex, DueUnit, Position, SimTick};
use campfire_stats::UnitStats;
use campfire_units::{Acting, Body, Dead, OnWings, StatusTags, Walk};
use campfire_values::{ActionId, Team, UnitType};

use crate::attack_move::AttackMove;
use crate::retaliate::Retaliate;

/// The destinations units walk to on their own: after their attack targets.
#[derive(Debug)]
pub struct Destinations;

impl Destinations {
    /// Walks each unit that can move to its attack target while out of the range of the weapon
    /// combat chose against it (combat-response.md, The chosen weapon is the attack's), and
    /// straight away from it to halfway between the weapon's minimum and its range while nearer
    /// than its minimum, of those the index of the attack targets holds, and
    /// stops it in reach or in its windup, but a unit on wings, which flies on at its target; a
    /// cast that walks in range walks the unit instead. A unit whose target is gone, dead, no
    /// longer an enemy or one it chose no weapon against drops it and stops, and so does one, that
    /// walks or stands, whose scan gave it a target out of that reach, which it does not chase
    /// (04-capabilities/control.md, A scanned target holds only in reach;
    /// 04-capabilities/actions.md, An action may keep a minimum range). A retaliation's target is
    /// chased as an ordered one until it ends, and its unit then attack-moves back to where it
    /// stood.
    pub(super) fn chase(
        (tick, book, retaliates, bonuses): (
            Res<'_, SimTick>,
            Res<'_, ActionBook>,
            Res<'_, ById<UnitType, Retaliate>>,
            Option<Res<'_, ById<ActionId, RangeBonus>>>,
        ),
        targets: Targets<'_, '_>,
        mut chasers: Query<
            '_,
            '_,
            (
                (&Position, &Team, &UnitType),
                (&ActionSlots, &mut ActionProgress),
                (Option<Walk>, Option<&mut AttackMove>),
                (Option<&Body>, Option<&StatusTags>, Option<&UnitStats>),
            ),
            (Without<Dead>, Acting),
        >,
        (mut indexes, mut goals, mut held): (
            ProgressIndexes<'_>,
            ResMut<'_, DueIndex<AttackMove>>,
            Local<'_, Vec<DueUnit>>,
        ),
    ) {
        let now = tick.start();
        let metric = targets.metric();
        held.clear();
        indexes.attackers(&mut held);
        for &DueUnit { id, entity } in &*held {
            let Ok((
                (&position, &team, unit_type),
                (slots, mut progress),
                (mut walk, attack_move),
                (body, status, stats),
            )) = chasers.get_mut(entity)
            else {
                continue;
            };
            // A unit of a turret target alone walks on, its turret aiming at it.
            let Some(target) = progress.attack_target() else {
                if progress.turret_target().is_none() {
                    indexes.forget_attacker(entity);
                }
                continue;
            };
            if progress.attacking().is_some() || progress.is_approaching() {
                continue;
            }
            let stretch = Stretch {
                bonuses: bonuses.as_deref(),
                stats: stats.map_or(&[][..], UnitStats::values),
            };
            let aimed = targets.enemy(team, target).and_then(|unit| {
                let values = stretch.values_in(slots, &book, progress.weapon()?)?;
                Some((unit, values))
            });
            // A retaliation ends as its chase's time passes, its target lies past its reach from
            // where its unit stood, or its unit past its leash, and so does one of no target: its
            // unit walks back by an attack-move (combat-response.md, A retaliation's target is
            // chased to its leash).
            if let Some(TargetSource::Retaliation { from, until }) = progress.target_source() {
                let retaliate = retaliates.get(*unit_type);
                let past = |bound: Num, at: Vec3| {
                    metric.order(Placed::point(from), bound, Placed::point(at)) == Ordering::Greater
                };
                let ended = now >= until
                    || aimed.is_none_or(|(unit, _)| {
                        retaliate.is_some_and(|retaliate| past(retaliate.reach, unit.position))
                    })
                    || retaliate.is_some_and(|retaliate| past(retaliate.leash, position.get()));
                if ended {
                    progress.set_attack_target(None, &mut indexes.of(id, entity));
                    if let Some(walk) = &mut walk {
                        walk.to(Some(from));
                    }
                    if let Some(mut attack_move) = attack_move {
                        attack_move.set_if_neq(AttackMove::to(Some(from)));
                        goals.push(Tick::ZERO, id, entity);
                    }
                    continue;
                }
            }
            let from = Placed::new(position.get(), Body::shape_of(body));
            let to = if let Some((unit, values)) = aimed {
                let to = Placed::new(unit.position, unit.shape);
                match targets.reach(from, values.range, values.min_range, to) {
                    // A unit on wings flies on at its target in reach, to pass it and come round
                    // (04-capabilities/combat.md, A jet aims by flying).
                    RangeFit::Within if OnWings::holds(status) => Some(unit.position),
                    RangeFit::Within => None,
                    _ if progress.target_source() == Some(TargetSource::Scan) => {
                        progress.set_attack_target(None, &mut indexes.of(id, entity));
                        None
                    }
                    fit => targets.walk_to(from, to, &values, fit),
                }
            } else {
                progress.set_attack_target(None, &mut indexes.of(id, entity));
                None
            };
            if let Some(walk) = &mut walk {
                walk.to(to);
            }
        }
    }
}
