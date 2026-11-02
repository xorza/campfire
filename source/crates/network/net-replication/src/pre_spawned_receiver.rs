use bevy_ecs::error;
use bevy_ecs::prelude::*;
use bevy_replicon::prelude::Signature;
use campfire_net_core::NetTick;
use campfire_net_despawns::DespawnReplicated;
use tracing::debug;

use crate::pre_spawned::PreSpawned;

/// The unmatched pre-spawned entities of a receiver, which expire when the server's applied state
/// reaches their tick unmatched. Entity matching itself is handled by Replicon's [`Signature`],
/// and prediction lists them for its rollbacks.
#[derive(Resource, Debug, Default)]
pub struct PreSpawnedReceiver {
    /// Each unmatched pre-spawned entity at the tick its hash holds, sorted by it: the tick of the
    /// server's simulation that would spawn its match, which no shift of the local timeline moves.
    pub(crate) unmatched: Vec<(NetTick, Entity)>,
}

impl PreSpawnedReceiver {
    /// Registers an entity that gained `PreSpawned` at the simulated `tick`: it is not listed,
    /// as its removal of `PreSpawned` took it off the list.
    pub(crate) fn register_unmatched_entity(&mut self, tick: NetTick, entity: Entity) {
        debug_assert!(
            !self
                .unmatched
                .iter()
                .any(|(_, candidate)| *candidate == entity),
            "an entity gains PreSpawned once before it loses it"
        );
        let at = self
            .unmatched
            .partition_point(|(listed, _)| *listed <= tick);
        self.unmatched.insert(at, (tick, entity));
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
        receiver.forget_unmatched(trigger.entity);
    }

    /// Takes `entity` off the unmatched list, as its match came or it is no longer pre-spawned.
    pub(crate) fn forget_unmatched(&mut self, entity: Entity) {
        if let Some(at) = self
            .unmatched
            .iter()
            .position(|(_, listed)| *listed == entity)
        {
            self.unmatched.remove(at);
        }
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

        /// Lists `entity` as pre-spawned at `tick`, as its `PreSpawned` does on a receiver.
        pub fn register(&mut self, tick: NetTick, entity: Entity) {
            self.register_unmatched_entity(tick, entity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
