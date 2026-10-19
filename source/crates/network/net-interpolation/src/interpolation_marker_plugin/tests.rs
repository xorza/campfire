use crate::internals::interpolation_stack::InterpolationStack;
use crate::internals::protocol::*;
use bevy::prelude::*;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_connection::{Connect, NetworkTarget};
use campfire_net_replication::{
    ConfirmedHistory, Interpolated, InterpolatedSend, InterpolationTarget, Replicate,
};
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::{Init, StepperConfig};
use campfire_net_stepper::{Order, Stepper};

fn host_only_stepper_before_host_connect() -> Stepper<InterpolationStack> {
    let mut config = StepperConfig::with_clients(vec![ClientKind::Host]);
    config.init = Init::Deferred;
    let mut stepper = Stepper::new(InterpolationStack, config);
    stepper.start_server();
    stepper
}

fn connect_host(stepper: &mut Stepper<InterpolationStack>) -> Entity {
    let host_client_entity = stepper.host_client_entity();
    stepper.server_app_mut().world_mut().trigger(Connect {
        entity: host_client_entity,
    });
    stepper.server_app_mut().world_mut().flush();
    host_client_entity
}

#[test]
fn a_manual_interpolated_marker_backfills_an_existing_replicated_component() {
    let mut stepper = Stepper::new(InterpolationStack, StepperConfig::single());
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            CompCustomInterp(3.0),
        ))
        .id();
    stepper.frame_step(2);

    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity was not replicated to client");
    assert!(
        stepper
            .client_app(0)
            .world()
            .entity(client_entity)
            .contains::<CompCustomInterp>()
    );

    stepper
        .client_app_mut(0)
        .world_mut()
        .entity_mut(client_entity)
        .insert(Interpolated);
    // History initialization is handled by the interpolation system for the
    // entity's newly matched rule, rather than synchronously by an observer.
    stepper.frame_step(1);

    let client_entity_ref = stepper.client_app(0).world().entity(client_entity);
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
fn interpolation_target_removal_removes_receiver_marker() {
    let mut stepper = Stepper::new(InterpolationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(2);

    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .unwrap();
    assert!(
        stepper
            .client_app(0)
            .world()
            .entity(client_entity)
            .contains::<Interpolated>()
    );

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_entity)
        .remove::<InterpolationTarget>();
    stepper.frame_step(2);

    assert!(
        !stepper
            .client_app(0)
            .world()
            .entity(client_entity)
            .contains::<Interpolated>()
    );
}

/// In host-server mode, spawning an entity with `InterpolationTarget` should
/// add the `Interpolated` marker component.
#[test]
fn an_interpolation_target_adds_interpolated_on_a_host_server() {
    let mut stepper = Stepper::new(InterpolationStack, StepperConfig::host_server());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(1);

    let entity_ref = stepper.server_app().world().entity(server_entity);
    assert!(
        entity_ref.contains::<InterpolatedSend>(),
        "InterpolationTarget should add the sender-side marker"
    );
    assert!(
        entity_ref.contains::<Interpolated>(),
        "entity should have Interpolated in host-server mode"
    );
}

#[test]
fn interpolation_target_backfills_interpolated_when_client_becomes_host_client() {
    let mut stepper = host_only_stepper_before_host_connect();

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(1);

    assert!(
        !stepper
            .server_app()
            .world()
            .entity(server_entity)
            .contains::<Interpolated>(),
        "the authoritative entity should not be Interpolated before it has a host-local receiver"
    );
    assert!(
        stepper
            .server_app()
            .world()
            .entity(server_entity)
            .contains::<InterpolatedSend>(),
        "InterpolationTarget should retain its sender-side marker"
    );

    connect_host(&mut stepper);
    stepper.frame_step(1);

    assert!(
        stepper
            .server_app()
            .world()
            .entity(server_entity)
            .contains::<Interpolated>(),
        "Interpolated should be added after the client becomes a HostClient"
    );
}

#[test]
fn a_child_overrides_the_prediction_target() {
    let mut stepper = Stepper::new(InterpolationStack, StepperConfig::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.step(1, stepper.frame_interval(), Order::ServerFirst);
    let client_entity = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity is not present in entity map");

    let server_child = stepper
        .server_app_mut()
        .world_mut()
        // the child should not be interpolated; it has InterpolationTarget, which takes precedence
        // over the one from the root entity
        .spawn((ChildOf(server_entity), InterpolationTarget::to_links([])))
        .id();
    stepper.step(1, stepper.frame_interval(), Order::ServerFirst);
    let client_child = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_child)
        .copied()
        .expect("entity is not present in entity map");
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<ChildOf>(client_child)
            .unwrap()
            .parent(),
        client_entity
    );
    // the parent is interpolated, but not the child
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Interpolated>(client_child)
            .is_none()
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Interpolated>(client_entity)
            .is_some()
    );
}
