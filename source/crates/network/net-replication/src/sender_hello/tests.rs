use crate::fixtures::replication_stack::ReplicationStack;
use crate::fixtures::steps::PROPAGATION_FRAMES;
use bevy_ecs::world::EntityRef;
use campfire_net_messages::LinkMessages;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

/// The server's hello maps its link entity to the client's own, in the client's messages layer's
/// map, as replicon's holds only replicated entities; the server, which maps nothing, keeps no
/// pair.
#[test]
fn the_servers_hello_maps_its_link_entity_on_the_client() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    // The hello goes on connection and arrives in the next frames.
    stepper.frame_step(PROPAGATION_FRAMES);
    let client = stepper.client_entity(0);
    let client_of = stepper.client_of_entity(0);
    let local = |entity: EntityRef<'_>, remote| {
        entity
            .get::<LinkMessages>()
            .unwrap()
            .entity_mapper
            .remote_to_local()
            .get(&remote)
            .copied()
    };
    assert_eq!(local(stepper.client(0), client_of), Some(client));
    assert_eq!(local(stepper.client_of(0), client), None);
}
