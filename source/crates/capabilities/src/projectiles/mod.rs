use bevy_ecs::entity::Entity;
use bevy_ecs::query::Without;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule};
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_sim::{IdAllocator, Position, SimSet, StateRegistry};

use crate::combat::CombatSet;
use crate::combat::attack_kind::AttackKind;
use crate::combat::damage::{Damage, DamageCause};
use crate::combat::damage_queue::DamageQueue;
use crate::combat::health::Health;
use crate::combat::launches::Launches;
use crate::combat::targets::Targets;
use crate::projectiles::projectile::Projectile;

pub(crate) mod projectile;

/// The `projectiles` capability: for now, the homing projectiles that ranged attacks fire. It
/// builds on combat, which a match installs too.
#[derive(Debug)]
pub struct Projectiles;

impl Projectiles {
    /// Adds projectiles to a match: a ranged attack fires one at the end of its windup instead of
    /// striking. In Hit, before attacks strike, each projectile flies a step towards its target,
    /// and strikes it on arrival, or ends without a hit once the target is dead or gone; after
    /// attacks strike, the tick's launches take off, and fly from the next tick.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.insert_resource(Launches::default());
        schedule.add_systems((
            fly.in_set(SimSet::Hit).before(CombatSet::Strike),
            launch.in_set(CombatSet::Launch),
        ));
        registry.register_component::<Projectile>();
    }
}

/// Flies each projectile a step towards its target's position now. One that reaches it strikes
/// and ends; one whose target is dead or gone ends without a hit.
fn fly(
    mut commands: Commands<'_, '_>,
    targets: Targets<'_, '_>,
    kind: Res<'_, AttackKind>,
    mut queue: ResMut<'_, DamageQueue>,
    mut projectiles: Query<'_, '_, (Entity, &mut Position, &Projectile), Without<Health>>,
) {
    for (entity, mut position, &projectile) in &mut projectiles {
        let Some(target) = targets.living(projectile.target()) else {
            commands.entity(entity).despawn();
            continue;
        };
        let moved = position
            .get()
            .step_toward(target.pos.get(), projectile.speed());
        if moved == target.pos.get() {
            queue.push(Damage {
                source: Some(projectile.source()),
                target: projectile.target(),
                amount: projectile.amount(),
                kind: kind.0,
                cause: DamageCause::Attack {
                    crit: projectile.crit(),
                },
                ability: None,
                depth: 0,
            });
            commands.entity(entity).despawn();
        } else {
            *position =
                Position::new(moved).expect("a step ends between two points within the bound");
        }
    }
}

/// Spawns the tick's launches, in the order of their source's stable id, so each takes the same
/// id in every run.
fn launch(
    mut commands: Commands<'_, '_>,
    mut ids: ResMut<'_, IdAllocator>,
    mut launches: ResMut<'_, Launches>,
) {
    launches.0.sort_unstable_by_key(|launch| launch.source);
    for launch in launches.0.drain(..) {
        let projectile = Projectile::new(
            launch.source,
            launch.target,
            launch.speed,
            launch.amount,
            launch.crit,
        )
        .expect("ranged attack stats hold a positive speed and damage that is not negative");
        commands.spawn((ids.allocate(), launch.from, projectile));
    }
}

#[cfg(test)]
mod tests;
