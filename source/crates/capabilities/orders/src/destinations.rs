use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res};
use campfire_actions::{
    ActionBook, ActionProgress, ActionSlots, ProgressIndexes, RangeFit, TargetSource, Targets,
};
use campfire_geometry::Placed;
use campfire_sim::{DueUnit, Position};
use campfire_units::{Body, Dead, Walk};
use campfire_values::Team;

/// The destinations units walk to on their own: after their attack targets.
#[derive(Debug)]
pub struct Destinations;

impl Destinations {
    /// Walks each unit that can move to its attack target while out of the range of the weapon it
    /// attacks it with, and straight away from it to halfway between the weapon's minimum and its
    /// range while nearer than its minimum, of those the index of the attack targets holds, and
    /// stops it in reach or in its windup; a cast that walks in range walks the unit instead. A
    /// unit whose target is gone, dead, no longer an enemy or one no weapon of it selects drops it
    /// and stops, and so does one, that walks or stands, whose scan gave it a target out of that
    /// reach, which it does not chase (04-capabilities/control.md, A scanned target holds only in
    /// reach; 04-capabilities/actions.md, An action may keep a minimum range).
    pub(super) fn chase(
        book: Res<'_, ActionBook>,
        targets: Targets<'_, '_>,
        mut chasers: Query<
            '_,
            '_,
            (
                &Position,
                &Team,
                (&ActionSlots, &mut ActionProgress),
                Option<Walk>,
                Option<&Body>,
            ),
            Without<Dead>,
        >,
        (mut indexes, mut held): (ProgressIndexes<'_>, Local<'_, Vec<DueUnit>>),
    ) {
        held.clear();
        indexes.attackers(&mut held);
        for &DueUnit { id, entity } in &*held {
            let Ok((&position, &team, (slots, mut progress), mut walk, body)) =
                chasers.get_mut(entity)
            else {
                continue;
            };
            let Some(target) = progress.attack_target() else {
                indexes.forget_attacker(entity);
                continue;
            };
            if progress.attacking().is_some() || progress.is_approaching() {
                continue;
            }
            let aimed = targets.enemy(team, target).and_then(|unit| {
                let selected = (targets.relation(team, unit.team), unit.tags);
                let slot = slots.weapon_in(&book, Some(selected))?;
                Some((unit, slots.values_in(&book, slot)?))
            });
            let from = Placed::new(position.get(), Body::shape_of(body));
            let to = if let Some((unit, values)) = aimed {
                let to = Placed::new(unit.position, unit.shape);
                match targets.reach(from, values.range, values.min_range, to) {
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
