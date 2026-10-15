use bevy_ecs::entity::Entity;
use bevy_ecs::query::{With, Without};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_actions::ActionSlots;
use campfire_sim::{Keyed, Ordered, Position, SimTick, StableId};
use campfire_stats::{Life, UnitPools};
use campfire_units::{Dead, Owner, SpawnPoint};
use campfire_values::Team;

use crate::deaths::{Deaths, Fallen};
use crate::kept::Kept;
use crate::on_death::OnDeath;
use crate::recent_attackers::RecentAttackers;
use crate::respawn::Respawn;

/// Units that die at zero life, the dead that despawn, and the dead whose respawn is due.
#[derive(Debug)]
pub struct Dying;

impl Dying {
    /// A unit at zero life dies: its attack target, and the action it ordered or has under way,
    /// end, so nothing it began goes on after a respawn, and the Mode stage learns of it; one no
    /// strike took there died with no killer. They die in stable-id order, which the record of
    /// deaths keeps for the hooks the mode runs and the deaths it carries to the next tick.
    pub(super) fn die(
        mut commands: Commands<'_, '_>,
        mut deaths: ResMut<'_, Deaths>,
        mut units: Query<
            '_,
            '_,
            (
                Entity,
                &StableId,
                &Life,
                Option<&mut ActionSlots>,
                Option<&Team>,
                Option<&Owner>,
            ),
            (With<OnDeath>, Without<Dead>),
        >,
        mut order: Local<'_, Ordered>,
    ) {
        deaths.index_killed();
        let dying = units
            .iter()
            .filter(|(_, _, life, ..)| life.is_empty())
            .map(|(entity, &id, ..)| Keyed { id, entity });
        for &Keyed { entity, id } in order.sort(dying) {
            let (_, _, _, slots, team, owner) =
                units.get_mut(entity).expect("a dying unit of the order");
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

    /// Brings back each dead unit whose respawn is due, at its spawn point with full pools.
    pub(super) fn respawn(
        tick: Res<'_, SimTick>,
        mut commands: Commands<'_, '_>,
        mut dead: Query<
            '_,
            '_,
            (Entity, &Respawn, &SpawnPoint, UnitPools, &mut Position),
            With<Dead>,
        >,
    ) {
        let now = tick.start();
        for (entity, respawn, spawn, mut pools, mut position) in &mut dead {
            if respawn.at > now {
                continue;
            }
            pools.fill();
            *position = Position::from(spawn.get());
            commands.entity(entity).remove::<(Dead, Respawn)>();
        }
    }

    /// Takes off the record each attacker of a dead unit whose respawn is due, so it comes back
    /// with none.
    pub(super) fn forget_attackers(
        tick: Res<'_, SimTick>,
        mut dead: Query<'_, '_, (&Respawn, &mut RecentAttackers), With<Dead>>,
    ) {
        let now = tick.start();
        for (respawn, mut attackers) in &mut dead {
            if respawn.at <= now {
                attackers.clear();
            }
        }
    }
}
