use crate::fixtures;
use crate::fixtures::AUDIT;
use crate::fixtures::LINK;
use campfire_net_connection::NetworkTarget;
use campfire_net_interpolation::internals::CompFull;
use campfire_net_replication::Replicate;
use campfire_net_stack::internals::FullStack;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::StepOrder;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

/// Each frame changes one replicated component on the server, which reaches the client in the
/// same frame. bevy_replicon grows its mutation message for the change (1).
#[test]
fn a_replicated_update_each_frame_stays_in_its_budget() {
    audit_replication(ClientKind::Plain);
}

/// The same in a sealed packet session of the UDP protocol.
#[test]
fn a_sealed_replicated_update_each_frame_stays_in_its_budget() {
    audit_replication(ClientKind::Udp);
}

#[track_caller]
fn audit_replication(kind: ClientKind) {
    let mut stepper = Stepper::new(FullStack, StepperSetup::with_clients(kind, 1));
    let server_entity = stepper
        .server_world_mut()
        .spawn((CompFull(0.0), Replicate::to_clients(NetworkTarget::All)))
        .id();
    let client_entity = fixtures::replicate_to_client(&mut stepper, server_entity);
    AUDIT.budget(LINK + 1).run(|| {
        let mut component = stepper
            .server_world_mut()
            .get_mut::<CompFull>(server_entity)
            .unwrap();
        component.0 += 1.0;
        let sent = component.0;
        stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);
        let received = stepper.client_world(0).get::<CompFull>(client_entity);
        assert_eq!(received, Some(&CompFull(sent)));
    });
}
