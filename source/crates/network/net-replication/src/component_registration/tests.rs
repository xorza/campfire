use super::*;
use crate::fixtures::protocol::{CompA, CompReplicateOnce, CompRepliconDiff};
use crate::fixtures::replication_stack::ReplicationStack;
use crate::fixtures::steps::{
    PROPAGATION_FRAMES, client_copy, on_client, on_server, spawn_on_server,
};
use crate::replication_target::Replicate;
use campfire_net_connection::NetworkTarget;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;

use crate::app_component_ext::AppComponentExt;

use bevy_ecs::component::Component;
use bevy_replicon::prelude::{AuthMethod, RepliconSharedPlugin};

use bevy_state::app::StatesPlugin;

#[derive(Debug, Component)]
struct LocalComponent;

fn replicon_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        StatesPlugin,
        RepliconSharedPlugin {
            auth_method: AuthMethod::None,
        },
    ));
    app
}

/// A registration with no replication method adds nothing to the protocol, so a client-only
/// component leaves the protocol hash both peers compare.
#[test]
fn registration_without_replication_leaves_the_protocol() {
    let mut app = App::new();
    app.register_component::<LocalComponent>();
    assert!(app.world().get_resource::<ComponentRegistry>().is_none());
}

/// The replication method records the kind that the later layers read.
#[test]
fn replication_records_its_kind() {
    let mut app = replicon_app();
    app.register_component::<CompA>().replicate();
    app.register_component::<CompRepliconDiff>()
        .replicate_diff();

    let registry = app.world().resource::<ComponentRegistry>();
    assert!(registry.is_registered::<CompA>());
    assert_eq!(
        registry.replication(&ComponentKind::of::<CompA>()),
        Some(Replication::Full)
    );
    assert_eq!(
        registry.replication(&ComponentKind::of::<CompRepliconDiff>()),
        Some(Replication::Diff)
    );

    app.register_component::<CompA>().already_replicated();
    app.register_component::<CompRepliconDiff>()
        .already_replicated_diff();
}

#[test]
#[should_panic(expected = "replicates both whole and as diffs")]
fn a_component_replicates_one_way() {
    let mut app = replicon_app();
    app.register_component::<CompRepliconDiff>().replicate();
    app.register_component::<CompRepliconDiff>()
        .replicate_diff();
}

#[test]
#[should_panic(expected = "has no earlier registration that replicates it this way")]
fn already_replicated_needs_an_earlier_replication() {
    let mut app = App::new();
    app.register_component::<CompA>().already_replicated();
}

#[test]
#[should_panic(expected = "has no earlier registration that replicates it this way")]
fn already_replicated_diff_needs_an_earlier_diff_replication() {
    let mut app = replicon_app();
    app.register_component::<CompRepliconDiff>().replicate();
    app.register_component::<CompRepliconDiff>()
        .already_replicated_diff();
}

#[test]
fn a_replicate_once_component_sends_only_its_insert() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperConfig::single());

        let source_entity = spawn_on_server(
            &mut stepper,
            (
                Replicate::to_clients(NetworkTarget::All),
                CompReplicateOnce(1.0),
            ),
        );
        stepper.frame_step(PROPAGATION_FRAMES);

        let mirrored_entity = client_copy(&stepper, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
        let initial_comp = on_client(&mut stepper, |world| {
            world
                .entity(mirrored_entity)
                .get::<CompReplicateOnce>()
                .cloned()
        });
        assert_eq!(
            initial_comp,
            Some(CompReplicateOnce(1.0)),
            "initial replicate_once component should replicate"
        );

        on_server(&mut stepper, |world| {
            world
                .entity_mut(source_entity)
                .get_mut::<CompReplicateOnce>()
                .unwrap()
                .0 = 2.0;
        });
        stepper.frame_step(PROPAGATION_FRAMES);

        let updated_comp = on_client(&mut stepper, |world| {
            world
                .entity(mirrored_entity)
                .get::<CompReplicateOnce>()
                .cloned()
        });
        assert_eq!(
            updated_comp,
            Some(CompReplicateOnce(1.0)),
            "replicate_once component updates should not replicate"
        );

        on_server(&mut stepper, |world| {
            world
                .entity_mut(source_entity)
                .remove::<CompReplicateOnce>();
        });
        stepper.frame_step(PROPAGATION_FRAMES);

        let removed = on_client(&mut stepper, |world| {
            world
                .entity(mirrored_entity)
                .get::<CompReplicateOnce>()
                .is_none()
        });
        assert!(removed, "replicate_once component remove should replicate");
    }
}
