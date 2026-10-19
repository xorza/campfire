use crate::confirmed_tick_map::ConfirmedTickMap;
use crate::pre_spawned::PreSpawned;
use crate::pre_spawned_receiver::PreSpawnedReceiver;
use crate::pre_spawned_systems::PreSpawnedSystems;
use crate::replication_receiver::ReplicationReceiver;
use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::prelude::*;
use bevy_replicon::client::confirm_history::ConfirmHistory;
use bevy_replicon::prelude::Signature;
use campfire_net_connection::Client;
use campfire_net_connection::Connected;
use campfire_net_connection::HostClient;
use campfire_net_timeline::LocalTimeline;

/// Matches an entity a client pre-spawned to the one its server replicates, by the hash of a
/// `PreSpawned` on each (13-net-stack/replication.md, An unmatched pre-spawn expires when the
/// server's applied state reaches its tick).
#[derive(Debug, Default)]
pub(crate) struct PreSpawnedPlugin;

impl Plugin for PreSpawnedPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreSpawnedReceiver>();
        app.configure_sets(PostUpdate, PreSpawnedSystems::CleanUp);
        app.add_observer(Self::register_pre_spawn);
        app.add_observer(Self::cleanup_matched_pre_spawn);
        app.add_observer(PreSpawnedReceiver::cleanup_removed_pre_spawn);
        app.add_observer(PreSpawnedReceiver::cleanup_despawned_pre_spawn);
        app.add_observer(PreSpawnedReceiver::handle_local_timeline_shift);
    }
}

impl PreSpawnedPlugin {
    /// For all newly added pre-spawns, register receiver-side lifecycle state
    /// and insert a Replicon Signature so incoming replicated entities can be
    /// matched to the local entity.
    fn register_pre_spawn(
        trigger: On<'_, '_, Add<PreSpawned>>,
        timeline: Res<'_, LocalTimeline>,
        query: Query<
            '_,
            '_,
            &PreSpawned,
            // Do not treat a replicated PreSpawned component that already has a
            // ConfirmHistory as a new local matching candidate.
            Without<ConfirmHistory>,
        >,
        connected_receivers: Query<
            '_,
            '_,
            (),
            (
                With<Client>,
                With<Connected>,
                With<ReplicationReceiver>,
                Without<HostClient>,
            ),
        >,
        mut receiver: ResMut<'_, PreSpawnedReceiver>,
        mut commands: Commands<'_, '_>,
    ) {
        let entity = trigger.entity;
        let Ok(pre_spawn) = query.get(entity) else {
            return;
        };
        // the hash can be None when PreSpawned is inserted, but the component
        // hook will calculate it, so it can't be None here.
        let hash = pre_spawn
            .hash()
            .expect("pre-spawn hash should have been calculated by a hook");

        // Only receiver-side pre-spawns need expiry and rollback bookkeeping. PreSpawnedPlugin
        // also runs in authoritative server worlds, which keep their entities.
        if !connected_receivers.is_empty() {
            let tick = timeline.simulated_tick();
            assert!(
                tick.is_some(),
                "a receiver pre-spawns {entity:?} outside FixedMain, where no tick is simulated: \
                 a rollback despawns it, and its replay makes it again only in FixedMain"
            );
            receiver.register_unmatched_entity(tick.unwrap(), entity);
        }

        let mut signature = Signature::from_hash(hash);
        if let Some(link) = pre_spawn.link {
            signature = signature.for_client(link);
        }
        commands.entity(entity).insert(signature);
    }

    /// Despawns the receiver's pre-spawned entities that the server state applied here reaches
    /// unmatched. Replicon applies a mutate message only once its update tick is applied, and it
    /// sends one each send, so the latest completed confirmed tick means that each entity the
    /// server spawned through its tick is here.
    pub(crate) fn expire_unmatched_pre_spawns(
        confirmed_ticks: Res<'_, ConfirmedTickMap>,
        mut receiver: ResMut<'_, PreSpawnedReceiver>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Some(confirmed) = confirmed_ticks.last_confirmed_tick() {
            receiver.expire_unmatched(confirmed, &mut commands);
        }
    }

    /// When a pre-spawned entity is matched with a server entity (ConfirmHistory added),
    /// update the PreSpawnedReceiver resource.
    fn cleanup_matched_pre_spawn(
        trigger: On<'_, '_, Add<ConfirmHistory>>,
        query: Query<'_, '_, (), With<PreSpawned>>,
        mut receiver: ResMut<'_, PreSpawnedReceiver>,
    ) {
        let entity = trigger.entity;
        if query.get(entity).is_ok() {
            receiver.forget_unmatched(entity);
            // Keep Signature attached for the rest of the entity lifetime.
            // Replicon removes SignatureMap during receive_replication, so
            // removing Signature during or shortly after the match can miss the
            // private map update and leave a stale hash -> entity entry. With
            // Signature kept on the matched entity, normal despawn always gives
            // Replicon a live entity whose on-remove hook can clear the hash.
        }
    }
}
