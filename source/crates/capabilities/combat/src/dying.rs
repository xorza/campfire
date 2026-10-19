use crate::kept::Kept;
use crate::on_death::OnDeath;
use crate::recent_attackers::RecentAttackers;
use crate::respawn::Respawn;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Has, With, Without};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_actions::{ActionProgress, ProgressIndexes};
use campfire_declare::ByType;
use campfire_deliveries::{Died, Teleported};
use campfire_sim::{DueIndex, DueUnit, Keyed, Ordered, Position, SimTick, StableId};
use campfire_stats::UnitPools;
use campfire_units::{Avatar, Dead, Life, Owner, SpawnPoint};
use campfire_units::{Deaths, Fallen};
use campfire_values::{Team, UnitType};

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
                Option<&mut ActionProgress>,
                Option<&Team>,
                Option<&Owner>,
                Has<Avatar>,
            ),
            (Without<Dead>, Changed<Life>),
        >,
        mut order: Local<'_, Ordered>,
        mut indexes: ProgressIndexes<'_>,
    ) {
        deaths.index_killed();
        let dying = units
            .iter()
            .filter(|(_, _, life, ..)| life.is_empty())
            .map(|(entity, &id, ..)| Keyed { id, entity });
        for &Keyed { entity, id } in order.sort(dying) {
            let (_, _, _, progress, team, owner, avatar) =
                units.get_mut(entity).expect("a dying unit of the order");
            if let Some(mut progress) = progress {
                let dues = &mut indexes.of(id, entity);
                progress.set_attack_target(None, dues);
                progress.stop(dues);
            }
            commands.entity(entity).insert(Dead);
            commands.trigger(Died { id });
            if !deaths.killed(id) {
                let fallen = Fallen {
                    id,
                    team: team.copied(),
                    owner: owner.map(|owner| owner.slot()),
                    avatar,
                };
                deaths.push(fallen, None, []);
            }
        }
    }

    /// Despawns the dead whose unit type despawns, at the end of the tick they died in, or of the
    /// tick a later stage stopped keeping them.
    pub(super) fn despawn_dead(
        mut commands: Commands<'_, '_>,
        deaths: Res<'_, ByType<OnDeath>>,
        dead: Query<'_, '_, (Entity, &UnitType), (With<Dead>, Without<Kept>)>,
    ) {
        for (entity, &unit_type) in &dead {
            if deaths.get(unit_type) == Some(&OnDeath::Despawn) {
                commands.entity(entity).despawn();
            }
        }
    }

    /// Brings back each dead unit whose respawn is due, at its spawn point with full pools: put
    /// there at once, as what homes on it learns. It visits only the units the index of the
    /// respawns due holds, by tick, then stable id, in a buffer it keeps.
    pub(super) fn respawn(
        tick: Res<'_, SimTick>,
        mut commands: Commands<'_, '_>,
        mut index: ResMut<'_, DueIndex<Respawn>>,
        mut dead: Query<'_, '_, (&Respawn, &SpawnPoint, UnitPools, &mut Position), With<Dead>>,
        mut due: Local<'_, Vec<DueUnit>>,
    ) {
        let now = tick.start();
        due.clear();
        index.take_due(now, &mut due);
        for &DueUnit { id, entity } in &*due {
            let Ok((respawn, spawn, mut pools, mut position)) = dead.get_mut(entity) else {
                continue;
            };
            if respawn.at > now {
                index.push(respawn.at, id, entity);
                continue;
            }
            pools.fill();
            *position = Position::from(spawn.get());
            commands.entity(entity).remove::<(Dead, Respawn)>();
            commands.trigger(Teleported { id });
        }
    }

    /// Takes off the record each attacker of a dead unit whose respawn is due, so it comes back
    /// with none: the units the index of the respawns due holds, before the respawn takes them.
    pub(super) fn forget_attackers(
        tick: Res<'_, SimTick>,
        index: Res<'_, DueIndex<Respawn>>,
        mut dead: Query<'_, '_, (&Respawn, &mut RecentAttackers), With<Dead>>,
    ) {
        let now = tick.start();
        for DueUnit { entity, .. } in index.peek_due(now) {
            if let Ok((respawn, mut attackers)) = dead.get_mut(entity)
                && respawn.at <= now
            {
                attackers.clear();
            }
        }
    }
}
