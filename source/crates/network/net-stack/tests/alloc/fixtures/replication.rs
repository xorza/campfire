use crate::fixtures;
use crate::fixtures::AUDIT;
use crate::fixtures::LINK;
use campfire_net_connection::NetworkTarget;
use campfire_net_interpolation::internals::protocol::CompFull;
use campfire_net_replication::Replicate;
use campfire_net_stack::internals::full_stack::FullStack;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Order;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;

/// Each frame changes one replicated component on the server, which reaches the client in the
/// same frame. bevy_replicon grows its mutation message for the change (1).
#[test]
fn replication_updates() {
    audit_replication(ClientKind::Raw);
}

/// The same in a sealed session of the UDP protocol.
#[test]
fn sealed_replication_updates() {
    audit_replication(ClientKind::Udp);
}

#[track_caller]
fn audit_replication(kind: ClientKind) {
    let mut stepper = Stepper::new(FullStack, StepperConfig::with_clients(vec![kind]));
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((CompFull(0.0), Replicate::to_clients(NetworkTarget::All)))
        .id();
    let client_entity = fixtures::replicate_to_client(&mut stepper, server_entity);
    AUDIT.budget(LINK + 1).run(|| {
        let mut component = stepper
            .server_app_mut()
            .world_mut()
            .get_mut::<CompFull>(server_entity)
            .unwrap();
        component.0 += 1.0;
        let sent = component.0;
        stepper.step(1, stepper.frame_interval(), Order::ServerFirst);
        let received = stepper.client_app(0).world().get::<CompFull>(client_entity);
        assert_eq!(received, Some(&CompFull(sent)));
    });
}
