use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Commands, Query, Res, ResMut};
use campfire_sim::{Position, SimTick, StableId};

use crate::actions::action_slots::ActionSlots;
use crate::combat::deaths::{Deaths, Fallen};
use crate::combat::kept::Kept;
use crate::combat::on_death::OnDeath;
use crate::combat::recent_attackers::RecentAttackers;
use crate::combat::respawn::Respawn;
use crate::stats::life_pool::LifePool;
use crate::stats::pools::Pools;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::spawn_point::SpawnPoint;
use crate::units::team::Team;

/// Units that die at zero life, the dead that despawn, and the dead whose respawn is due.
#[derive(Debug)]
pub(super) struct Dying;

impl Dying {
    /// A unit at zero life dies: its attack target, and the action it ordered or has under way,
    /// end, so nothing it began goes on after a respawn, and the Mode stage learns of it; one no
    /// strike took there died with no killer.
    pub(super) fn die(
        mut commands: Commands<'_, '_>,
        life: Res<'_, LifePool>,
        mut deaths: ResMut<'_, Deaths>,
        mut units: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                &Pools,
                Option<&mut ActionSlots>,
                Option<&Team>,
                Option<&Owner>,
            ),
            (With<OnDeath>, Without<Dead>),
        >,
    ) {
        deaths.index_killed();
        for (entity, &id, pools, slots, team, owner) in &mut units {
            if pools.above_zero(life.0) {
                continue;
            }
            if let Some(mut slots) = slots {
                slots.set_attack_target(None);
                slots.stop();
            }
            commands.entity(entity).insert(Dead);
            if !deaths.killed(id) {
                deaths.push(Fallen::of(id, team, owner), None, []);
            }
        }
    }

    /// Despawns the dead whose unit type despawns, at the end of the tick they died in, or of the
    /// tick a later stage stopped keeping them.
    pub(super) fn despawn_dead(
        mut commands: Commands<'_, '_>,
        dead: Query<'_, '_, (Entity, &OnDeath), (With<Dead>, Without<Kept>)>,
    ) {
        for (entity, &on_death) in &dead {
            if on_death == OnDeath::Despawn {
                commands.entity(entity).despawn();
            }
        }
    }

    /// Brings back each dead unit whose respawn is due, at its spawn point with full pools and no
    /// one on record as its attacker.
    pub(super) fn respawn(
        tick: Res<'_, SimTick>,
        mut commands: Commands<'_, '_>,
        mut dead: Query<
            '_,
            '_,
            (
                Entity,
                &Respawn,
                &SpawnPoint,
                &mut Pools,
                &mut Position,
                Option<&mut RecentAttackers>,
            ),
            With<Dead>,
        >,
    ) {
        let now = tick.start();
        for (entity, respawn, spawn, mut pools, mut position, attackers) in &mut dead {
            if respawn.at > now {
                continue;
            }
            pools.fill();
            *position = spawn.get();
            if let Some(mut attackers) = attackers {
                attackers.clear();
            }
            commands.entity(entity).remove::<(Dead, Respawn)>();
        }
    }
}
