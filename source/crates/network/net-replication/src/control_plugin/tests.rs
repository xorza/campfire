use super::*;
use crate::controlled_send::ControlledSend;
use crate::internals::replication_stack::ReplicationStack;
use crate::lifetime::Lifetime;
use crate::replication_target::Replicate;
use crate::server_send_plugin::ServerSendPlugin;
use bevy::prelude::*;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_connection::{Connect, NetworkTarget};
use campfire_net_despawn::DespawnReplicated;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::{Init, StepperConfig};

/// When a client disconnects, entities it controlled (via ControlledBy with SessionBased lifetime)
/// should be despawned on the server.
#[test]
fn a_session_controlled_entity_is_despawned_when_its_client_disconnects() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::with_raw_clients(2));

    let client_of_1 = stepper.client_of(1).id();
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ControlledBy {
                link: client_of_1,
                lifetime: Lifetime::default(),
            },
        ))
        .id();
    assert!(stepper.client_of(1).get::<ControlledByRemote>().is_some());

    // the server entity is replicated to both clients
    stepper.frame_step(2);
    stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&server_entity)
        .copied()
        .expect("entity is not present in entity map");

    // client 1 disconnects
    stepper.disconnect(1);
    stepper.frame_step(2);

    // the entity controlled by client 1 should be despawned on the server
    assert!(
        stepper
            .server_app()
            .world()
            .get_entity(server_entity)
            .is_err(),
        "Entity controlled by disconnected client should be despawned on the server"
    );
}

/// `Lifetime::Independent` keeps a controlled entity alive on the sender when its controller
/// disconnects.
#[test]
fn an_independent_controlled_entity_survives_its_client_disconnect() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::with_raw_clients(2));

    let client_of_1 = stepper.client_of(1).id();
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ControlledBy {
                link: client_of_1,
                lifetime: Lifetime::Independent,
            },
        ))
        .id();
    stepper.frame_step(2);

    // Disconnect and remove client 1's link entity.
    stepper.disconnect(1);
    stepper.frame_step(2);

    assert!(
        stepper
            .server_app()
            .world()
            .get_entity(server_entity)
            .is_ok(),
        "Entity with independent lifetime should remain on the sender"
    );
}

/// Spawning a host-owned entity with `ControlledBy` should add the sender
/// marker and the host-local receiver marker.
#[test]
fn a_host_owned_controlled_by_adds_local_controlled() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::host_server());

    let host_client_entity = stepper.host_client_entity();
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ControlledBy {
                link: host_client_entity,
                lifetime: Lifetime::default(),
            },
        ))
        .id();
    stepper.frame_step(1);

    let entity_ref = stepper.server_app().world().entity(server_entity);
    assert!(
        entity_ref.contains::<ControlledSend>(),
        "entity should have ControlledSend via #[require(ControlledSend)] on ControlledBy"
    );
    assert!(
        entity_ref.contains::<Controlled>(),
        "host-owned entity should have host-local Controlled in host-server mode"
    );
}

#[test]
fn a_remote_owned_controlled_by_adds_no_local_controlled_on_a_host_server() {
    let mut stepper = Stepper::new(ReplicationStack, StepperConfig::host_server());

    let remote_client_entity = stepper.client_of_entity(0);
    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ControlledBy {
                link: remote_client_entity,
                lifetime: Lifetime::default(),
            },
        ))
        .id();
    stepper.frame_step(1);

    let entity_ref = stepper.server_app().world().entity(server_entity);
    assert!(
        entity_ref.contains::<ControlledSend>(),
        "entity should have ControlledSend via #[require(ControlledSend)] on ControlledBy"
    );
    assert!(
        !entity_ref.contains::<Controlled>(),
        "remote-owned server entity should not have host-local Controlled"
    );
}

#[test]
fn controlled_backfills_when_a_client_becomes_the_host_client() {
    let mut stepper = host_only_stepper_before_host_connect();
    let host_client_entity = stepper.host_client_entity();

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            ControlledBy {
                link: host_client_entity,
                lifetime: Lifetime::default(),
            },
        ))
        .id();
    stepper.frame_step(1);

    assert!(
        stepper
            .server_app()
            .world()
            .entity(server_entity)
            .contains::<ControlledSend>(),
        "ControlledSend is added by ControlledBy before the host-local observer runs"
    );
    assert!(
        !stepper
            .server_app()
            .world()
            .entity(server_entity)
            .contains::<Controlled>(),
        "Controlled should not be present before the link becomes a HostClient"
    );

    connect_host(&mut stepper);
    stepper.frame_step(1);

    assert!(
        stepper
            .server_app()
            .world()
            .entity(server_entity)
            .contains::<Controlled>(),
        "Controlled should still be present after the client becomes a HostClient"
    );
}

fn host_only_stepper_before_host_connect() -> Stepper<ReplicationStack> {
    let mut config = StepperConfig::with_clients(vec![ClientKind::Host]);
    config.init = Init::Deferred;
    let mut stepper = Stepper::new(ReplicationStack, config);
    stepper.start_server();
    stepper
}

fn connect_host(stepper: &mut Stepper<ReplicationStack>) -> Entity {
    let host_client_entity = stepper.host_client_entity();
    stepper.server_app_mut().world_mut().trigger(Connect {
        entity: host_client_entity,
    });
    stepper.server_app_mut().world_mut().flush();
    host_client_entity
}

/// A link that goes without a disconnect, as a despawned link does, takes its session-based
/// entities with it and leaves its independent ones.
#[test]
fn a_link_despawn_ends_its_session() {
    let mut app = App::new();
    app.add_observer(ControlledBy::end_link_session);
    let link = app.world_mut().spawn_empty().id();
    let [session, independent] = [Lifetime::SessionBased, Lifetime::Independent]
        .map(|lifetime| app.world_mut().spawn(ControlledBy { link, lifetime }).id());

    DespawnReplicated.apply(app.world_mut().entity_mut(link));
    app.world_mut().flush();

    assert!(app.world().get_entity(session).is_err());
    assert!(app.world().get_entity(independent).is_ok());
}

/// On a host server, an entity is `Controlled` exactly while a host client owns it: a new link
/// that is remote takes the marker, and the link's end of its host role takes it too.
#[test]
fn controlled_follows_a_host_link() {
    let mut app = App::new();
    app.add_plugins((ServerSendPlugin, ControlPlugin));
    let host = app
        .world_mut()
        .spawn(HostClient { buffer: Vec::new() })
        .id();
    let remote = app.world_mut().spawn_empty().id();
    let entity = app
        .world_mut()
        .spawn(ControlledBy {
            link: host,
            lifetime: Lifetime::default(),
        })
        .id();
    app.world_mut().flush();
    let controlled = |app: &App| app.world().entity(entity).contains::<Controlled>();
    assert!(controlled(&app));

    app.world_mut().entity_mut(entity).insert(ControlledBy {
        link: remote,
        lifetime: Lifetime::default(),
    });
    app.world_mut().flush();
    assert!(!controlled(&app));

    app.world_mut().entity_mut(entity).insert(ControlledBy {
        link: host,
        lifetime: Lifetime::default(),
    });
    app.world_mut().flush();
    assert!(controlled(&app));

    app.world_mut().entity_mut(host).remove::<HostClient>();
    app.world_mut().flush();
    assert!(!controlled(&app));
}
