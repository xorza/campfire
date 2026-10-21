use super::*;
use crate::controlled_by::ControlledByRemote;
use crate::controlled_lifetime::ControlledLifetime;
use crate::fixtures::replication_stack::ReplicationStack;
use crate::fixtures::steps::ClientCopies;
use crate::fixtures::steps::PROPAGATION_FRAMES;
use crate::replication_target::Replicate;
use bevy_ecs::system::EntityCommand;
use campfire_net_connection::NetworkTarget;
use campfire_net_despawns::DespawnReplicated;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

/// When a client disconnects, entities it controlled (via ControlledBy with SessionBased lifetime)
/// should be despawned on the server.
#[test]
fn a_link_lifetime_entity_is_despawned_when_its_client_disconnects() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    let client_of_1 = stepper.client_of(1).id();
    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ControlledBy {
                link: client_of_1,
                lifetime: ControlledLifetime::default(),
            },
        ))
        .id();
    assert!(stepper.client_of(1).get::<ControlledByRemote>().is_some());

    // the server entity is replicated to both clients
    stepper.frame_step(PROPAGATION_FRAMES);
    stepper
        .client_copy(0, server_entity)
        .expect("entity is not present in entity map");

    // client 1 disconnects
    stepper.disconnect(1);
    stepper.frame_step(PROPAGATION_FRAMES);

    // the entity controlled by client 1 should be despawned on the server
    assert!(
        stepper.server_world().get_entity(server_entity).is_err(),
        "Entity controlled by disconnected client should be despawned on the server"
    );
}

/// `ControlledLifetime::Independent` keeps a controlled entity alive on the sender when its
/// controller disconnects.
#[test]
fn an_independent_controlled_entity_survives_its_client_disconnect() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    let client_of_1 = stepper.client_of(1).id();
    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ControlledBy {
                link: client_of_1,
                lifetime: ControlledLifetime::Independent,
            },
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    // Disconnect and remove client 1's link entity.
    stepper.disconnect(1);
    stepper.frame_step(PROPAGATION_FRAMES);

    assert!(
        stepper.server_world().get_entity(server_entity).is_ok(),
        "Entity with independent lifetime should remain on the sender"
    );
}

/// A link that goes without a disconnect, as a despawned link does, takes its entities of the
/// `Link` lifetime with it and leaves its independent ones.
#[test]
fn a_link_despawn_takes_its_link_lifetime_entities() {
    let mut app = App::new();
    app.add_observer(ControlledBy::end_with_link);
    let link = app.world_mut().spawn_empty().id();
    let [packet_session, independent] = [ControlledLifetime::Link, ControlledLifetime::Independent]
        .map(|lifetime| app.world_mut().spawn(ControlledBy { link, lifetime }).id());

    DespawnReplicated.apply(app.world_mut().entity_mut(link));
    app.world_mut().flush();

    assert!(app.world().get_entity(packet_session).is_err());
    assert!(app.world().get_entity(independent).is_ok());
}
