use bevy_ecs::query::Without;
use bevy_ecs::system::{Local, Query, Res};
use campfire_actions::{
    ActionBook, ActionProgress, ActionRange, ActionSlots, ProgressIndexes, Targets,
};
use campfire_geometry::Placed;
use campfire_sim::{DueUnit, Position};
use campfire_units::{ArrivedShort, Body, Dead, Destination};
use campfire_values::Team;

/// The destinations units walk to on their own: after their attack targets.
#[derive(Debug)]
pub struct Destinations;

impl Destinations {
    /// Walks each unit that can move to its attack target while out of the range of the weapon it
    /// attacks it with, of those the index of the attack targets holds, and stops it in range or
    /// in its windup; a cast that walks in range walks the unit instead. A unit whose target is
    /// gone, dead, no longer an enemy or one no weapon of it selects drops it and stops.
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
                &mut Destination,
                Option<&ArrivedShort>,
                Option<&Body>,
            ),
            Without<Dead>,
        >,
        (mut indexes, mut held): (ProgressIndexes<'_>, Local<'_, Vec<DueUnit>>),
    ) {
        held.clear();
        held.extend(indexes.attackers());
        for &DueUnit { id, entity } in &*held {
            let Ok((&position, &team, (slots, mut progress), mut destination, arrived, body)) =
                chasers.get_mut(entity)
            else {
                continue;
            };
            let Some(target) = progress.attack_target() else {
                indexes.forget_attacker(id);
                continue;
            };
            if progress.attacking().is_some() || progress.approaching() {
                continue;
            }
            let aimed = targets.enemy(team, target).and_then(|unit| {
                let selected = (targets.relation(team, unit.team), unit.tags);
                let slot = slots.weapon_in(&book, Some(selected))?;
                Some((unit, slots.range_in(&book, slot)))
            });
            match aimed {
                None => {
                    progress.set_attack_target(None, &mut indexes.of(id, entity));
                    Destination::walk_to(&mut destination, arrived, None);
                }
                Some((unit, ActionRange::Meters(range)))
                    if !targets.reaches(
                        Placed::new(position.get(), Body::shape_of(body)),
                        range.get(),
                        &unit,
                    ) =>
                {
                    Destination::walk_to(&mut destination, arrived, Some(unit.pos));
                }
                Some(_) => Destination::walk_to(&mut destination, arrived, None),
            }
        }
    }
}
