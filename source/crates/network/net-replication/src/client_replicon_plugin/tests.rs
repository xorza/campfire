use super::*;
use crate::fixtures::protocol::CompA;
use crate::fixtures::replication_stack::ReplicationStack;
use crate::fixtures::steps::ClientCopies;
use crate::fixtures::steps::PROPAGATION_FRAMES;
use crate::replication_target::Replicate;
use bevy_replicon::client::server_mutate_ticks::ServerMutateTicks;
use campfire_net_connection::NetworkTarget;
use campfire_net_core::NetTick;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

/// When a client disconnects, all replicated entities on that client should be despawned.
#[test]
fn a_disconnect_despawns_the_replicated_entities_on_the_client() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    // server spawns an entity replicated to all clients
    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    // verify the entity exists on client 0
    let client_entity = stepper
        .client_copy(0, server_entity)
        .expect("entity should be replicated to client 0");
    assert!(
        stepper
            .client_world(0)
            .get::<Remote>(client_entity)
            .is_some(),
        "Entity should have Remote component"
    );

    // disconnect client 1 (the last one), this pops client 1
    stepper.disconnect(1);
    stepper.frame_step(PROPAGATION_FRAMES);

    // client 0 should still have the entity (it didn't disconnect)
    assert!(
        stepper.client_world(0).get_entity(client_entity).is_ok(),
        "Entity should still exist on client 0 since it didn't disconnect"
    );
}

/// When a client disconnects, entities that were replicated from the server
/// should be despawned on that client (via replicon's ClientState transition).
#[test]
fn a_disconnecting_client_despawns_every_replicated_entity() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    // server spawns an entity replicated to all clients
    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    // verify the entity exists on client 1
    let client_entity_on_1 = stepper
        .client_copy(1, server_entity)
        .expect("entity should be replicated to client 1");
    assert!(
        stepper
            .client_world(1)
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
            .client_world(1)
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
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    let persistent_server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ChildOf(server_entity),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    let persistent_client_entity = stepper
        .client_copy(1, persistent_server_entity)
        .expect("persistent entity should be replicated to client 1");
    let client_entity = stepper
        .client_copy(1, server_entity)
        .expect("the parent should be replicated to client 1");
    stepper
        .client_world_mut(1)
        .entity_mut(persistent_client_entity)
        .insert(Persistent);

    stepper.disconnect(1);
    stepper.frame_step(3);

    assert!(
        stepper
            .client_world(1)
            .get_entity(persistent_client_entity)
            .is_ok(),
        "Persistent replicated entity should survive disconnect"
    );
    assert!(
        stepper.client_world(1).get_entity(client_entity).is_err(),
        "Non-persistent replicated entity should be despawned on disconnect"
    );
    let kept = stepper.client_world(1).entity(persistent_client_entity);
    assert!(!kept.contains::<Remote>());
    assert!(!kept.contains::<ConfirmHistory>());
    assert!(!kept.contains::<ChildOf>());
    assert!(!kept.contains::<RemoteParent>());

    // A reconnect replicates the server's entity anew, beside the local copy.
    stepper.reconnect(1);
    stepper.frame_step(10);
    let replicated_again = stepper
        .client_copy(1, persistent_server_entity)
        .expect("the server's entity replicates again after the reconnect");
    assert_ne!(replicated_again, persistent_client_entity);
    assert!(
        stepper
            .client_world(1)
            .entity(replicated_again)
            .contains::<Remote>()
    );
    assert!(
        stepper
            .client_world(1)
            .get_entity(persistent_client_entity)
            .is_ok()
    );
}

/// A `Persistent` marker on the receiver exempts all received entities from disconnect cleanup.
#[test]
fn a_persistent_receiver_keeps_its_entities_on_a_disconnect() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    let client_entity = stepper
        .client_copy(1, server_entity)
        .expect("entity should be replicated to client 1");
    let receiver = stepper.client_entity(1);
    stepper
        .client_world_mut(1)
        .entity_mut(receiver)
        .insert(Persistent);

    stepper.disconnect(1);
    stepper.frame_step(3);

    assert!(
        stepper.client_world(1).get_entity(client_entity).is_ok(),
        "Persistent receiver should preserve its replicated entities"
    );
}

/// Removing `ReplicationReceiver` performs the same cleanup as disconnecting its link.
#[test]
fn removing_the_receiver_cleans_up_as_a_disconnect_does() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

    let persistent_server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    let persistent_client_entity = stepper
        .client_copy(0, persistent_server_entity)
        .expect("persistent entity should be replicated to the client");
    let client_entity = stepper
        .client_copy(0, server_entity)
        .expect("the entity should be replicated to the client");
    let confirmed_tick = RepliconTick::new(123);
    let receiver = stepper.client_entity(0);
    {
        let world = stepper.client_world_mut(0);
        world
            .entity_mut(persistent_client_entity)
            .insert(Persistent);
        world
            .resource_mut::<ConfirmedTickMap>()
            .record(confirmed_tick, NetTick(456));
        world.entity_mut(receiver).remove::<ReplicationReceiver>();
        world.flush();
    }

    assert!(
        stepper
            .client_world(0)
            .get_entity(persistent_client_entity)
            .is_ok(),
        "Persistent replicated entity should survive receiver removal"
    );
    assert!(
        stepper.client_world(0).get_entity(client_entity).is_err(),
        "Non-persistent replicated entity should be despawned on receiver removal"
    );
    // The confirmed ticks clear with replicon's reset, in the next frame's state transition.
    stepper.frame_step(1);
    assert_eq!(
        stepper
            .client_world(0)
            .resource::<ConfirmedTickMap>()
            .get(confirmed_tick),
        None,
        "Receiver removal should clear the confirmed tick map"
    );

    stepper.frame_step(PROPAGATION_FRAMES);
    assert_eq!(
        stepper
            .client_world(0)
            .resource::<State<ClientState>>()
            .get(),
        &ClientState::Disconnected,
        "Removing ReplicationReceiver should reset Replicon's client state"
    );
}

/// Receiver persistence remains observable while the receiver entity itself is being despawned.
#[test]
fn despawning_a_persistent_receiver_keeps_its_entities() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    let client_entity = stepper
        .client_copy(0, server_entity)
        .expect("entity should be replicated to the client");
    let confirmed_tick = RepliconTick::new(123);
    let receiver = stepper.client_entity(0);
    {
        let world = stepper.client_world_mut(0);
        world
            .resource_mut::<ConfirmedTickMap>()
            .record(confirmed_tick, NetTick(456));
        world.entity_mut(receiver).insert(Persistent);
        DespawnReplicated.apply(world.entity_mut(receiver));
        world.flush();
    }

    assert!(
        stepper.client_world(0).get_entity(client_entity).is_ok(),
        "Persistent receiver should preserve entities while it is despawned"
    );
    // The confirmed ticks clear with replicon's reset, in the next frame's state transition.
    stepper.frame_step(1);
    assert_eq!(
        stepper
            .client_world(0)
            .resource::<ConfirmedTickMap>()
            .get(confirmed_tick),
        None,
        "Receiver despawn should clear the confirmed tick map"
    );
}

/// `Persistent` only controls disconnect cleanup; sender-replicated despawns still apply.
#[test]
fn a_persistent_entity_honors_a_remote_despawn() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);
    let client_entity = stepper
        .client_copy(0, server_entity)
        .expect("entity should be replicated to the client");
    stepper
        .client_world_mut(0)
        .entity_mut(client_entity)
        .insert(Persistent);

    DespawnReplicated.apply(stepper.server_world_mut().entity_mut(server_entity));
    stepper.frame_step(PROPAGATION_FRAMES);

    assert!(
        stepper.client_world(0).get_entity(client_entity).is_err(),
        "An explicitly replicated despawn should still despawn a persistent entity"
    );
}

/// The stack's receive set holds each system of replicon's, which applies the received
/// replication.
#[test]
fn the_receive_set_holds_replicons_receive() {
    let stepper = Stepper::new(ReplicationStack, StepperSetup::single());
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

/// A link that fails and comes back at once starts its replication afresh: replicon's reset and
/// the confirmed ticks' clear run in one transition, so the client never holds a confirmed tick of
/// the old link that the new one did not map, and the server's mutations reach it again.
#[test]
fn a_link_that_comes_back_at_once_starts_its_replication_afresh() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All), CompA(0.0)))
        .id();
    let mutate = |stepper: &mut Stepper<ReplicationStack>, value: f32| {
        stepper
            .server_world_mut()
            .entity_mut(server_entity)
            .insert(CompA(value));
        stepper.frame_step(PROPAGATION_FRAMES);
    };
    let held = |stepper: &Stepper<ReplicationStack>| {
        let copy = stepper.client_copy(0, server_entity)?;
        stepper.client_world(0).get::<CompA>(copy).cloned()
    };
    stepper.frame_step(PROPAGATION_FRAMES);
    for value in [1.0, 2.0] {
        mutate(&mut stepper, value);
    }
    assert_eq!(held(&stepper), Some(CompA(2.0)));
    assert!(
        stepper
            .client_world(0)
            .resource::<ConfirmedTickMap>()
            .last_confirmed_tick()
            .is_some()
    );

    stepper.disconnect(0);
    stepper.reconnect(0);
    stepper.frame_step(10);
    for value in [3.0, 4.0] {
        mutate(&mut stepper, value);
    }
    assert_eq!(held(&stepper), Some(CompA(4.0)));
    let world = stepper.client_world(0);
    let replicon = world.resource::<ServerMutateTicks>().last_confirmed_tick();
    let mapped = world.resource::<ConfirmedTickMap>().last_confirmed();
    assert_eq!(mapped.map(|confirmed| confirmed.replicon_tick), replicon);
}
