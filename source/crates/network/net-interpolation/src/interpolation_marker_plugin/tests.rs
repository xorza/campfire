use crate::fixtures::interpolation_stack::InterpolationStack;
use crate::fixtures::protocol::*;
use bevy_ecs::hierarchy::ChildOf;
use campfire_net_connection::NetworkTarget;
use campfire_net_replication::internals::ClientCopies;
use campfire_net_replication::internals::PROPAGATION_FRAMES;
use campfire_net_replication::{ConfirmedHistory, Interpolated, InterpolationTarget, Replicate};
use campfire_net_stepper::StepperSetup;
use campfire_net_stepper::{StepOrder, Stepper};

#[test]
fn a_manual_interpolated_marker_backfills_an_existing_replicated_component() {
    let mut stepper = Stepper::new(InterpolationStack, StepperSetup::single());
    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            CompCustomInterp(3.0),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    let client_entity = stepper
        .client_copy(0, server_entity)
        .expect("entity was not replicated to client");
    assert!(
        stepper
            .client_world(0)
            .entity(client_entity)
            .contains::<CompCustomInterp>()
    );

    stepper
        .client_world_mut(0)
        .entity_mut(client_entity)
        .insert(Interpolated);
    // History initialization is handled by the interpolation system for the
    // entity's newly matched rule, rather than synchronously by an observer.
    stepper.frame_step(1);

    let client_entity_ref = stepper.client_world(0).entity(client_entity);
    assert_eq!(
        client_entity_ref
            .get::<ConfirmedHistory<CompCustomInterp>>()
            .and_then(ConfirmedHistory::newest_present)
            .map(|entry| entry.value),
        Some(&CompCustomInterp(3.0)),
        "manual interpolation should seed history from the existing replicated value"
    );
    assert!(
        !client_entity_ref.contains::<CompCustomInterp>(),
        "the live value should wait for the interpolation timeline after manual opt-in"
    );
}

#[test]
fn the_removal_of_an_interpolation_target_removes_the_receiver_marker() {
    let mut stepper = Stepper::new(InterpolationStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    let client_entity = stepper.client_copy(0, server_entity).unwrap();
    assert!(
        stepper
            .client_world(0)
            .entity(client_entity)
            .contains::<Interpolated>()
    );

    stepper
        .server_world_mut()
        .entity_mut(server_entity)
        .remove::<InterpolationTarget>();
    stepper.frame_step(PROPAGATION_FRAMES);

    assert!(
        !stepper
            .client_world(0)
            .entity(client_entity)
            .contains::<Interpolated>()
    );
}

#[test]
fn a_child_overrides_the_prediction_target() {
    let mut stepper = Stepper::new(InterpolationStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);
    let client_entity = stepper
        .client_copy(0, server_entity)
        .expect("entity is not present in entity map");

    let server_child = stepper
        .server_world_mut()
        // the child should not be interpolated; it has InterpolationTarget, which takes precedence
        // over the one from the root entity
        .spawn((ChildOf(server_entity), InterpolationTarget::to_links([])))
        .id();
    stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);
    let client_child = stepper
        .client_copy(0, server_child)
        .expect("entity is not present in entity map");
    assert_eq!(
        stepper
            .client_world(0)
            .get::<ChildOf>(client_child)
            .unwrap()
            .parent(),
        client_entity
    );
    // the parent is interpolated, but not the child
    assert!(
        stepper
            .client_world(0)
            .get::<Interpolated>(client_child)
            .is_none()
    );
    assert!(
        stepper
            .client_world(0)
            .get::<Interpolated>(client_entity)
            .is_some()
    );
}
