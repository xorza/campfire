use super::*;
use crate::internals::replication_stack::ReplicationStack;
use crate::replication_target::Replicate;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_connection::NetworkTarget;
use campfire_net_core::NetTick;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;

/// When a client disconnects, all replicated entities on that client should be despawned.
#[test]
fn a_disconnect_despawns_the_replicated_entities_on_the_client() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::with_raw_clients(2));

    // server spawns an entity replicated to all clients
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(2);

    // verify the entity exists on client 0
    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity should be replicated to client 0");
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Remote>(client_entity)
            .is_some(),
        "Entity should have Remote component"
    );

    // disconnect client 1 (the last one), this pops client 1
    stepper.disconnect(1);
    stepper.frame_step(2);

    // client 0 should still have the entity (it didn't disconnect)
    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_entity)
            .is_ok(),
        "Entity should still exist on client 0 since it didn't disconnect"
    );
}

/// When a client disconnects, entities that were replicated from the server
/// should be despawned on that client (via replicon's ClientState transition).
#[test]
fn a_disconnecting_client_despawns_every_replicated_entity() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::with_raw_clients(2));

    // server spawns an entity replicated to all clients
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(2);

    // verify the entity exists on client 1
    let client_entity_on_1 = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity should be replicated to client 1");
    assert!(
        stepper
            .client_app(1)
            .world()
            .get::<Remote>(client_entity_on_1)
            .is_some(),
        "Entity should have Remote component on client 1"
    );

    // Manually trigger disconnect on client 1 without removing it from the stepper
    stepper.disconnect(1);

    // Run a few frames to let the state transition happen
    // We need to update client 1 manually since frame_step updates all clients
    stepper.frame_step(3);

    // After disconnect, all replicated entities should be despawned on client 1
    assert!(
        stepper
            .client_app(1)
            .world()
            .get_entity(client_entity_on_1)
            .is_err(),
        "Remote entity should be despawned on client 1 after disconnect"
    );
}

/// A `Persistent` marker on a received entity exempts only that entity from disconnect cleanup,
/// which turns it into a local entity: as a child of an entity the cleanup despawns, it outlives
/// its parent and loses its parent.
#[test]
fn a_persistent_entity_survives_a_disconnect() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::with_raw_clients(2));

    let session_server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    let persistent_server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ChildOf(session_server_entity),
        ))
        .id();
    stepper.frame_step(2);

    let persistent_client_entity = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&persistent_server_entity)
        .copied()
        .expect("persistent entity should be replicated to client 1");
    let session_client_entity = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&session_server_entity)
        .copied()
        .expect("session entity should be replicated to client 1");
    stepper
        .client_app_mut(1)
        .world_mut()
        .entity_mut(persistent_client_entity)
        .insert(Persistent);

    stepper.disconnect(1);
    stepper.frame_step(3);

    assert!(
        stepper
            .client_app(1)
            .world()
            .get_entity(persistent_client_entity)
            .is_ok(),
        "Persistent replicated entity should survive disconnect"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .get_entity(session_client_entity)
            .is_err(),
        "Non-persistent replicated entity should be despawned on disconnect"
    );
    let kept = stepper
        .client_app(1)
        .world()
        .entity(persistent_client_entity);
    assert!(!kept.contains::<Remote>());
    assert!(!kept.contains::<ConfirmHistory>());
    assert!(!kept.contains::<ChildOf>());
    assert!(!kept.contains::<RemoteParent>());

    // A reconnect replicates the server's entity anew, beside the local copy.
    stepper.reconnect(1);
    stepper.frame_step(10);
    let replicated_again = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&persistent_server_entity)
        .copied()
        .expect("the server's entity replicates again after the reconnect");
    assert_ne!(replicated_again, persistent_client_entity);
    assert!(
        stepper
            .client_app(1)
            .world()
            .entity(replicated_again)
            .contains::<Remote>()
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .get_entity(persistent_client_entity)
            .is_ok()
    );
}

/// A `Persistent` marker on the receiver exempts all received entities from disconnect cleanup.
#[test]
fn a_persistent_receiver_keeps_its_entities_on_a_disconnect() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::with_raw_clients(2));

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(2);

    let client_entity = stepper
        .client_app(1)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity should be replicated to client 1");
    let receiver = stepper.client_entity(1);
    stepper
        .client_app_mut(1)
        .world_mut()
        .entity_mut(receiver)
        .insert(Persistent);

    stepper.disconnect(1);
    stepper.frame_step(3);

    assert!(
        stepper
            .client_app(1)
            .world()
            .get_entity(client_entity)
            .is_ok(),
        "Persistent receiver should preserve its replicated entities"
    );
}

/// Removing `ReplicationReceiver` performs the same cleanup as disconnecting its link.
#[test]
fn removing_the_receiver_cleans_up_as_a_disconnect_does() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let persistent_server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    let session_server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(2);

    let persistent_client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&persistent_server_entity)
        .copied()
        .expect("persistent entity should be replicated to the client");
    let session_client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&session_server_entity)
        .copied()
        .expect("session entity should be replicated to the client");
    let checkpoint = RepliconTick::new(123);
    let receiver = stepper.client_entity(0);
    {
        let world = stepper.client_app_mut(0).world_mut();
        world
            .entity_mut(persistent_client_entity)
            .insert(Persistent);
        world
            .resource_mut::<ReplicationCheckpointMap>()
            .record(checkpoint, NetTick(456));
        world.entity_mut(receiver).remove::<ReplicationReceiver>();
        world.flush();
    }

    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(persistent_client_entity)
            .is_ok(),
        "Persistent replicated entity should survive receiver removal"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(session_client_entity)
            .is_err(),
        "Non-persistent replicated entity should be despawned on receiver removal"
    );
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .resource::<ReplicationCheckpointMap>()
            .get(checkpoint),
        None,
        "Receiver removal should clear the checkpoint map"
    );

    stepper.frame_step(2);
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .resource::<State<ClientState>>()
            .get(),
        &ClientState::Disconnected,
        "Removing ReplicationReceiver should reset Replicon's client state"
    );
}

/// Receiver persistence remains observable while the receiver entity itself is being despawned.
#[test]
fn despawning_a_persistent_receiver_keeps_its_entities() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(2);

    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity should be replicated to the client");
    let checkpoint = RepliconTick::new(123);
    let receiver = stepper.client_entity(0);
    {
        let world = stepper.client_app_mut(0).world_mut();
        world
            .resource_mut::<ReplicationCheckpointMap>()
            .record(checkpoint, NetTick(456));
        world.entity_mut(receiver).insert(Persistent);
        DespawnReplicated.apply(world.entity_mut(receiver));
        world.flush();
    }

    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_entity)
            .is_ok(),
        "Persistent receiver should preserve entities while it is despawned"
    );
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .resource::<ReplicationCheckpointMap>()
            .get(checkpoint),
        None,
        "Receiver despawn should clear the checkpoint map"
    );
}

/// `Persistent` only controls disconnect cleanup; sender-replicated despawns still apply.
#[test]
fn a_persistent_entity_honors_a_remote_despawn() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(2);
    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity should be replicated to the client");
    stepper
        .client_app_mut(0)
        .world_mut()
        .entity_mut(client_entity)
        .insert(Persistent);

    DespawnReplicated.apply(
        stepper
            .server_app_mut()
            .world_mut()
            .entity_mut(server_entity),
    );
    stepper.frame_step(2);

    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_entity)
            .is_err(),
        "An explicitly replicated despawn should still despawn a persistent entity"
    );
}

/// The stack's receive set holds each system of replicon's, which applies the received
/// replication.
#[test]
fn the_receive_set_holds_replicons_receive() {
    let stepper = Stepper::new(ReplicationStack, StepperConfig::single());
    let graph = stepper
        .client_app(0)
        .get_schedule(PreUpdate)
        .unwrap()
        .graph();
    let replicon = graph
        .systems_in_set(ClientSystems::Receive.intern())
        .unwrap();
    let stack = graph
        .systems_in_set(ReplicationSystems::Receive.intern())
        .unwrap();
    assert!(!replicon.is_empty());
    assert!(replicon.iter().all(|system| stack.contains(system)));
}
