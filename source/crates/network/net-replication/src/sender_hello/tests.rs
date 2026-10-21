use crate::fixtures::replication_stack::ReplicationStack;
use crate::fixtures::steps::PROPAGATION_FRAMES;
use bevy_ecs::world::EntityRef;
use campfire_net_messages::LinkMessages;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

/// Each end's hello maps the peer's link entity to its own, in the messages layer's map:
/// replicon's map holds only replicated entities.
#[test]
fn hello_maps_the_link_entities_both_ways() {
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
            .get_local(remote)
    };
    assert_eq!(local(stepper.client_of(0), client), Some(client_of));
    assert_eq!(local(stepper.client(0), client_of), Some(client));
}
