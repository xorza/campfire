use bevy_ecs::error;
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::Signature;
use campfire_net_core::NetTick;
use campfire_net_despawns::DespawnReplicated;
use campfire_net_timeline::LocalTimelineShift;
use tracing::debug;

use crate::pre_spawned::PreSpawned;

/// Global lifecycle state for locally pre-spawned entities.
///
/// Tracks locally pre-spawned entities for rollback and for the expiry of those the server never
/// sent. Entity matching itself is handled by Replicon's [`Signature`].
#[derive(Resource, Debug, Default)]
pub struct PreSpawnedReceiver {
    /// Each pre-spawned entity, matched or not, at its spawn tick on the local timeline, which
    /// moves with it, so that a rollback despawns those its replay makes again. Sorted by the
    /// tick, and only those a rollback can still reach.
    pub(crate) rollback: Vec<(NetTick, Entity)>,
    /// Each unmatched pre-spawned entity at the tick its hash holds, sorted by it: the tick of the
    /// server's simulation that would spawn its match, which no shift of the local timeline moves.
    pub(crate) unmatched: Vec<(NetTick, Entity)>,
}

impl PreSpawnedReceiver {
    /// Registers an entity that gained `PreSpawned` at the simulated `tick`: it is not listed,
    /// as its removal of `PreSpawned` took it off the lists.
    pub(crate) fn register_unmatched_entity(&mut self, tick: NetTick, entity: Entity) {
        debug_assert!(
            !self
                .unmatched
                .iter()
                .any(|(_, candidate)| *candidate == entity),
            "an entity gains PreSpawned once before it loses it"
        );
        insert_sorted(&mut self.rollback, tick, entity);
        insert_sorted(&mut self.unmatched, tick, entity);
    }

    /// Despawn all local PreSpawned entities spawned at a tick >= NetTick,
    /// except entities that the caller marks as protected from this rollback.
    ///
    /// One-shot entities can be pre-spawned and later matched by Replicon, but
    /// not recreated by rollback replay. Prediction uses this to keep
    /// `LocallyPredicted { skip_despawn: true }` entities alive during a forced
    /// rollback.
    #[doc(hidden)]
    pub fn despawn_pre_spawned_after_with(
        &mut self,
        tick: NetTick,
        should_keep: impl Fn(Entity) -> bool,
        commands: &mut Commands<'_, '_>,
    ) {
        let start = self
            .rollback
            .partition_point(|(spawn_tick, _)| *spawn_tick < tick);
        let mut kept = start;
        for index in start..self.rollback.len() {
            let entry = self.rollback[index];
            let entity = entry.1;
            if should_keep(entity) {
                self.rollback[kept] = entry;
                kept += 1;
                continue;
            }
            debug!(
                ?entity,
                "deleting pre-spawned entity because it was created after the rollback tick"
            );
            if let Ok(mut entity_commands) = commands.get_entity(entity) {
                entity_commands.queue_handled(DespawnReplicated, error::warn);
            }
        }
        self.rollback.truncate(kept);
    }

    /// Forgets the pre-spawned entities spawned at `oldest` or before: a rollback restores
    /// `oldest` at the earliest and despawns only what spawned after the tick it restores.
    #[doc(hidden)]
    pub fn forget_rollback_through(&mut self, oldest: NetTick) {
        let reached = self
            .rollback
            .partition_point(|(spawn_tick, _)| *spawn_tick <= oldest);
        self.rollback.drain(..reached);
    }

    /// Despawns each unmatched pre-spawned entity whose tick the server's applied state reaches:
    /// a match of it would have been spawned through `confirmed`, and none came.
    pub(crate) fn expire_unmatched(&mut self, confirmed: NetTick, commands: &mut Commands<'_, '_>) {
        let expired = self
            .unmatched
            .partition_point(|(tick, _)| *tick <= confirmed);
        for (tick, entity) in self.unmatched.drain(..expired) {
            if let Ok(mut entity_commands) = commands.get_entity(entity) {
                debug!(
                    ?tick,
                    ?entity,
                    ?confirmed,
                    "despawning a pre-spawned entity the server state through its tick did not match"
                );
                entity_commands.queue_handled(DespawnReplicated, error::warn);
            }
        }
    }

    pub(crate) fn handle_local_timeline_shift(
        trigger: On<'_, '_, LocalTimelineShift>,
        mut receiver: ResMut<'_, Self>,
    ) {
        receiver
            .rollback
            .iter_mut()
            .for_each(|(tick, _)| *tick = *tick + trigger.delta);
    }

    pub(crate) fn cleanup_removed_pre_spawn(
        trigger: On<'_, '_, Remove<PreSpawned>>,
        mut receiver: ResMut<'_, Self>,
    ) {
        receiver.forget_unmatched(trigger.entity);
    }

    pub(crate) fn cleanup_despawned_pre_spawn(
        trigger: On<'_, '_, Despawn<(Signature, PreSpawned)>>,
        mut receiver: ResMut<'_, Self>,
    ) {
        let entity = trigger.entity;
        receiver.forget_unmatched(entity);
        remove_entity(&mut receiver.rollback, entity);
    }

    /// Takes `entity` off the unmatched list, as its match came or it is no longer pre-spawned.
    pub(crate) fn forget_unmatched(&mut self, entity: Entity) {
        remove_entity(&mut self.unmatched, entity);
    }
}

/// Inserts `entity` at `tick` after the entries at the same tick, so `list` stays sorted.
fn insert_sorted(list: &mut Vec<(NetTick, Entity)>, tick: NetTick, entity: Entity) {
    let at = list.partition_point(|(listed, _)| *listed <= tick);
    list.insert(at, (tick, entity));
}

/// Removes `entity`'s entry, listed at most once, from `list`, keeping its order.
fn remove_entity(list: &mut Vec<(NetTick, Entity)>, entity: Entity) {
    if let Some(index) = list.iter().position(|(_, listed)| *listed == entity) {
        list.remove(index);
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::pre_spawned_receiver::PreSpawnedReceiver;
    use bevy_ecs::entity::Entity;
    use campfire_net_core::NetTick;

    impl PreSpawnedReceiver {
        /// The unmatched pre-spawned entities, by the tick their hash holds.
        pub fn unmatched(&self) -> &[(NetTick, Entity)] {
            &self.unmatched
        }

        /// Every pre-spawned entity, matched or not, by its spawn tick on the local timeline.
        pub fn rollback(&self) -> &[(NetTick, Entity)] {
            &self.rollback
        }

        /// Lists `entity` as pre-spawned at `tick`, as its `PreSpawned` does on a receiver.
        pub fn register(&mut self, tick: NetTick, entity: Entity) {
            self.register_unmatched_entity(tick, entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rollback to tick 10 despawns the pre-spawned entities spawned at 10 or later, matched or
    /// not, except the one `should_keep` names; those before 10 stay, and the list keeps only what
    /// it did not despawn.
    #[test]
    fn a_rollback_despawns_the_pre_spawns_from_its_tick_except_the_kept() {
        let mut world = World::new();
        let [early, at, late, kept, matched_early, matched_late] =
            core::array::from_fn(|_| world.spawn_empty().id());
        let mut receiver = PreSpawnedReceiver {
            rollback: vec![
                (NetTick(5), early),
                (NetTick(9), matched_early),
                (NetTick(10), at),
                (NetTick(12), matched_late),
                (NetTick(15), late),
                (NetTick(15), kept),
            ],
            unmatched: Vec::new(),
        };
        let mut commands = world.commands();
        receiver.despawn_pre_spawned_after_with(
            NetTick(10),
            |entity| entity == kept,
            &mut commands,
        );
        world.flush();

        let alive = |entity| world.get_entity(entity).is_ok();
        assert_eq!(
            [early, at, late, kept, matched_early, matched_late].map(alive),
            [true, false, false, true, true, false]
        );
        assert_eq!(
            receiver.rollback,
            [
                (NetTick(5), early),
                (NetTick(9), matched_early),
                (NetTick(15), kept)
            ]
        );

        // A rollback restores tick 9 at the earliest: the entries at 9 and before go, and
        // the registration of a late spawn at 7 takes its place in tick order.
        receiver.forget_rollback_through(NetTick(9));
        assert_eq!(receiver.rollback, [(NetTick(15), kept)]);
        receiver.register_unmatched_entity(NetTick(7), early);
        assert_eq!(
            receiver.rollback,
            [(NetTick(7), early), (NetTick(15), kept)]
        );
    }

    /// A shift of the local timeline by 1000 ticks moves the rollback list with it, and not the
    /// unmatched list, whose ticks are the hash's: the expiry waits for the same server tick.
    #[test]
    fn a_timeline_shift_moves_the_rollback_ticks_only() {
        let mut world = World::new();
        let entity = world.spawn_empty().id();
        let mut receiver = PreSpawnedReceiver::default();
        receiver.register_unmatched_entity(NetTick(10), entity);
        world.insert_resource(receiver);
        world.add_observer(PreSpawnedReceiver::handle_local_timeline_shift);

        world.trigger(LocalTimelineShift { delta: 1000 });

        let receiver = world.resource::<PreSpawnedReceiver>();
        assert_eq!(receiver.rollback, [(NetTick(1010), entity)]);
        assert_eq!(receiver.unmatched, [(NetTick(10), entity)]);
    }

    /// The server state applied through tick 10 expires the unmatched pre-spawns at 10 and before,
    /// 9 and 10, and keeps 11; an entity registered late at 8 takes its place in tick order.
    #[test]
    fn the_applied_server_state_expires_the_unmatched_pre_spawns_it_reaches() {
        let mut world = World::new();
        let [nine, ten, eleven, eight] = core::array::from_fn(|_| world.spawn_empty().id());
        let mut receiver = PreSpawnedReceiver::default();
        receiver.register_unmatched_entity(NetTick(9), nine);
        receiver.register_unmatched_entity(NetTick(10), ten);
        receiver.register_unmatched_entity(NetTick(11), eleven);
        receiver.register_unmatched_entity(NetTick(8), eight);
        assert_eq!(
            receiver.unmatched,
            [
                (NetTick(8), eight),
                (NetTick(9), nine),
                (NetTick(10), ten),
                (NetTick(11), eleven)
            ]
        );

        let mut commands = world.commands();
        receiver.expire_unmatched(NetTick(10), &mut commands);
        world.flush();

        let alive = |entity| world.get_entity(entity).is_ok();
        assert_eq!(
            [eight, nine, ten, eleven].map(alive),
            [false, false, false, true]
        );
        assert_eq!(receiver.unmatched, [(NetTick(11), eleven)]);
    }
}
