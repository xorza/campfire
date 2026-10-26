use super::*;
use crate::fixtures::protocol::*;
use crate::fixtures::replication_counts::summed_replication_counts;
use crate::fixtures::replication_stack::ReplicationStack;
use crate::fixtures::steps::ClientCopies;
use crate::fixtures::steps::PROPAGATION_FRAMES;
use crate::interpolated::Interpolated;
use crate::predicted::Predicted;
use crate::replication_mode::ReplicationMode;
use crate::replication_sender::ReplicationSender;
use crate::targets_overlap::TargetsOverlap;
use crate::visibility_ext::VisibilityExt;
use campfire_net_channels::internals::ChannelCounts;
use campfire_net_channels::internals::TransportCounts;
use campfire_net_connection::NetworkTarget;
use campfire_net_connection::RemoteId;
use campfire_net_despawns::DespawnReplicated;
use campfire_net_stepper::StepOrder;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_transport::internals::summed_transport_counts;

#[test]
fn a_replicated_spawn_reaches_the_client() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_clients(NetworkTarget::All),))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);

        stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
    }
}

#[test]
fn a_replicate_insert_spawns_the_entity_on_the_client() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_links([]),))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);
        assert!(
            stepper.client_copy(0, source_entity).is_none(),
            "entity should not be replicated yet"
        );

        {
            let world = stepper.server_world_mut();
            world
                .entity_mut(source_entity)
                .insert(Replicate::to_clients(NetworkTarget::All));
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
    }
}

/// When client 2 connects:
/// - the existing entities are replicated to the new client
#[test]
fn a_new_client_receives_the_existing_entities() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);
    stepper.client_copy(0, server_entity).unwrap();

    // second client connects
    stepper.add_remote_client();
    stepper.init();
    stepper.frame_step(PROPAGATION_FRAMES);

    // make sure the entity is also replicated to the newly connected client
    stepper
        .client_copy(1, server_entity)
        .expect("entity is not present in entity map");
}

/// When client 2 connects:
/// - existing entities outside its replication target are not replicated to it
#[test]
fn a_new_client_receives_only_the_entities_that_target_it() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    let client_0_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;

    let server_entity = stepper
        .server_world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::single(client_0_id)),))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);
    stepper
        .client_copy(0, server_entity)
        .expect("entity is not present in target client entity map");

    // second client connects
    stepper.add_remote_client();
    stepper.init();
    stepper.frame_step(PROPAGATION_FRAMES);

    // make sure the entity is not replicated to the newly connected client
    assert!(
        stepper.client_copy(1, server_entity).is_none(),
        "entity is present in non-target client entity map"
    );
}

#[test]
fn a_link_without_a_replication_sender_receives_no_replication() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    // Drop the marker from the second client's link: it stays connected
    // (messages still flow) but must receive no replication.
    let sender_1 = stepper.client_of_entity(1);
    stepper
        .server_world_mut()
        .entity_mut(sender_1)
        .remove::<ReplicationSender>();

    let server_entity = stepper
        .server_world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    assert!(
        stepper.client_copy(0, server_entity).is_some(),
        "entity is not present in sending client entity map"
    );
    assert!(
        stepper.client_copy(1, server_entity).is_none(),
        "entity is present in markerless client entity map"
    );

    // Re-adding the marker admits the link; pre-existing entities replicate.
    stepper
        .server_world_mut()
        .entity_mut(sender_1)
        .insert(ReplicationSender);
    stepper.frame_step(PROPAGATION_FRAMES);

    assert!(
        stepper.client_copy(1, server_entity).is_some(),
        "entity is not present in re-admitted client entity map"
    );
}

/// A link that loses its sender after replicating is detached: its client despawns what it held
/// and receives nothing spawned meanwhile. A new sender replicates every entity anew, and the
/// manual visibility set before the detach still hides its entity.
#[test]
fn a_detached_link_respawns_its_entities_and_keeps_its_manual_visibility() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    let link = stepper.client_of_entity(0);
    let shown = stepper
        .server_world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    let hidden = stepper
        .server_world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    stepper.server_world_mut().lose_visibility(hidden, link);
    stepper.frame_step(PROPAGATION_FRAMES);
    assert!(stepper.client_copy(0, shown).is_some());
    assert!(stepper.client_copy(0, hidden).is_none());

    {
        let world = stepper.server_world_mut();
        world.entity_mut(link).remove::<ReplicationSender>();
    };
    stepper.frame_step(PROPAGATION_FRAMES);
    let late = stepper
        .server_world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);
    assert_eq!(
        [shown, hidden, late].map(|entity| stepper.client_copy(0, entity).is_some()),
        [false, false, false]
    );

    {
        let world = stepper.server_world_mut();
        world.entity_mut(link).insert(ReplicationSender);
    };
    stepper.frame_step(PROPAGATION_FRAMES);
    assert_eq!(
        [shown, hidden, late].map(|entity| stepper.client_copy(0, entity).is_some()),
        [true, false, true]
    );
}

#[test]
fn a_target_replicates_to_the_links_it_names() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));
    let client_0_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;

    let server_entity = stepper
        .server_world_mut()
        .spawn(Replicate::new(ReplicationMode::Target(
            NetworkTarget::single(client_0_id),
        )))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    assert!(
        stepper.client_copy(0, server_entity).is_some(),
        "entity is not present in target client entity map"
    );
    assert!(
        stepper.client_copy(1, server_entity).is_none(),
        "entity is present in non-target client entity map"
    );
}

#[test]
fn a_replicated_despawn_reaches_the_client() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_clients(NetworkTarget::All),))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);
        let mirrored_entity = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
        let replicating = {
            let world = stepper.server_world_mut();
            world.entity(source_entity).contains::<Replicated>()
        };
        assert!(
            replicating,
            "Replicate should insert the required Replicated marker"
        );

        {
            let world = stepper.server_world_mut();
            DespawnReplicated.apply(world.entity_mut(source_entity));
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let mirrored_exists = {
            let world = stepper.client_world_mut(0);
            world.get_entity(mirrored_entity).is_ok()
        };
        assert!(!mirrored_exists, "entity should be despawned on the target");
    }
}

#[test]
fn a_replicate_retarget_despawns_the_entity_on_the_client() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_clients(NetworkTarget::All),))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);
        let mirrored_entity = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));

        {
            let world = stepper.server_world_mut();
            world
                .entity_mut(source_entity)
                .insert(Replicate::to_links([]));
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let mirrored_exists = {
            let world = stepper.client_world_mut(0);
            world.get_entity(mirrored_entity).is_ok()
        };
        assert!(
            !mirrored_exists,
            "entity should be despawned after replication target removal"
        );
    }
}

#[test]
fn a_component_insert_reaches_the_client() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_clients(NetworkTarget::All),))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);
        let mirrored_entity = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));

        {
            let world = stepper.server_world_mut();
            world.entity_mut(source_entity).insert(CompA(1.0));
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let mirrored_comp = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().cloned()
        };
        assert_eq!(
            mirrored_comp,
            Some(CompA(1.0)),
            "component insert should replicate"
        );
    }
}

#[test]
fn a_component_update_reaches_the_client() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_clients(NetworkTarget::All), CompA(1.0)))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);
        let mirrored_entity = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
        let initial_comp = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().cloned()
        };
        assert_eq!(
            initial_comp,
            Some(CompA(1.0)),
            "initial component state should replicate"
        );

        {
            let world = stepper.server_world_mut();
            world
                .entity_mut(source_entity)
                .get_mut::<CompA>()
                .unwrap()
                .0 = 2.0;
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let updated_comp = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().cloned()
        };
        assert_eq!(
            updated_comp,
            Some(CompA(2.0)),
            "component update should replicate"
        );
    }
}

/// Replicating updates works even after a large tick jump.
///
/// Exhausting a `u32` tick takes more than two years at 60 Hz, so this test only verifies that a
/// moderate jump (10 000 ticks) does not break replication.
#[test]
fn a_component_update_reaches_the_client_after_a_large_tick_jump() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_clients(NetworkTarget::All), CompA(1.0)))
            .id();

        stepper.frame_step(PROPAGATION_FRAMES);
        let mirrored_entity = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));

        // Apply a large tick jump on both client and server
        stepper
            .client_world_mut(0)
            .resource_mut::<LocalTimeline>()
            .shift(10_000);
        stepper
            .server_world_mut()
            .resource_mut::<LocalTimeline>()
            .shift(10_000);
        stepper.frame_step(PROPAGATION_FRAMES);

        stepper
            .client_world_mut(0)
            .resource_mut::<LocalTimeline>()
            .shift(10_000);
        stepper
            .server_world_mut()
            .resource_mut::<LocalTimeline>()
            .shift(10_000);
        stepper.frame_step(PROPAGATION_FRAMES);

        {
            let world = stepper.server_world_mut();
            world
                .entity_mut(source_entity)
                .get_mut::<CompA>()
                .unwrap()
                .0 = 2.0;
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let updated_comp = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().cloned()
        };
        assert_eq!(
            updated_comp,
            Some(CompA(2.0)),
            "component update should survive tick jump"
        );
    }
}

#[test]
fn a_component_remove_reaches_the_client() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_clients(NetworkTarget::All), CompA(1.0)))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);
        let mirrored_entity = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
        let initial_comp = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().cloned()
        };
        assert_eq!(
            initial_comp,
            Some(CompA(1.0)),
            "initial component state should replicate"
        );

        {
            let world = stepper.server_world_mut();
            world.entity_mut(source_entity).remove::<CompA>();
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let removed = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().is_none()
        };
        assert!(removed, "component remove should replicate");
    }
}

/// Check that component removes are not replicated while replication is paused.
#[test]
fn a_component_remove_is_not_replicated_while_replication_is_paused() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_clients(NetworkTarget::All), CompA(1.0)))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);
        let mirrored_entity = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));

        {
            let world = stepper.server_world_mut();
            world.entity_mut(source_entity).remove::<Replicated>();
            world.entity_mut(source_entity).remove::<CompA>();
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let retained_component = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().cloned()
        };
        assert_eq!(
            retained_component,
            Some(CompA(1.0)),
            "component remove should not replicate while paused"
        );

        {
            let world = stepper.server_world_mut();
            world.entity_mut(source_entity).insert(Replicated);
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let mapped_after_resume = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
        assert_eq!(
            mapped_after_resume, mirrored_entity,
            "resuming should reuse the remote entity"
        );

        {
            let world = stepper.server_world_mut();
            world.entity_mut(source_entity).insert(CompA(2.0));
        };
        stepper.frame_step(PROPAGATION_FRAMES);
        let resumed_component = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().cloned()
        };
        assert_eq!(
            resumed_component,
            Some(CompA(2.0)),
            "component inserts should replicate after resuming"
        );
    }
}

/// Removing the replication target changes visibility instead of pausing replication in place.
#[test]
fn the_removal_of_a_replication_target_despawns_the_remote_entity() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
        let source_entity = stepper
            .server_world_mut()
            .spawn((Replicate::to_clients(NetworkTarget::All),))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);
        let mirrored_entity = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));

        {
            let world = stepper.server_world_mut();
            world.entity_mut(source_entity).remove::<Replicate>();
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let remote_exists = {
            let world = stepper.client_world_mut(0);
            world.get_entity(mirrored_entity).is_ok()
        };
        assert!(
            !remote_exists,
            "removing Replicate should despawn the remote entity"
        );
        let replicating = {
            let world = stepper.server_world_mut();
            world.entity(source_entity).contains::<Replicated>()
        };
        assert!(
            !replicating,
            "removing Replicate should also remove Replicated"
        );
    }
}

/// Check that if we remove a non-replicated component, the replicate component does not get removed
#[test]
fn removing_a_non_replicated_component_keeps_the_replicated_ones() {
    {
        let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

        let source_entity = stepper
            .server_world_mut()
            .spawn((
                Replicate::to_clients(NetworkTarget::All),
                CompA(1.0),
                Name::from("a"),
            ))
            .id();
        stepper.frame_step(PROPAGATION_FRAMES);
        let mirrored_entity = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
        let initial_comp = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().cloned()
        };
        assert_eq!(
            initial_comp,
            Some(CompA(1.0)),
            "initial component state should replicate"
        );

        {
            let world = stepper.server_world_mut();
            world.entity_mut(source_entity).remove::<Name>();
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let comp_still_present = {
            let world = stepper.client_world_mut(0);
            world.entity(mirrored_entity).get::<CompA>().is_some()
        };
        assert!(
            comp_still_present,
            "removing a non-replicated component should not affect replicated state"
        );
    }
}

/// Removing `Replicated` before despawning keeps the despawn local to the sender.
#[test]
fn a_sender_can_despawn_with_no_despawn_replicated() {
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
        .server_world_mut()
        .entity_mut(server_entity)
        .remove::<Replicated>();
    DespawnReplicated.apply(stepper.server_world_mut().entity_mut(server_entity));
    stepper.frame_step(PROPAGATION_FRAMES);

    assert!(
        stepper.client_world(0).get_entity(client_entity).is_ok(),
        "Removing Replicated before despawning should suppress the remote despawn"
    );
}

const ENTITIES: usize = 1000;

/// The transport counters of a stepper's apps, with the replication channels' share of them.
#[derive(Debug, PartialEq, Eq)]
struct StepperCounts {
    transport: TransportCounts,
    replication: ChannelCounts,
}

impl core::ops::Sub for StepperCounts {
    type Output = Self;

    fn sub(self, base: Self) -> Self {
        Self {
            transport: self.transport - base.transport,
            replication: self.replication - base.replication,
        }
    }
}

impl StepperCounts {
    fn read(stepper: &mut Stepper<ReplicationStack>) -> Self {
        Self {
            transport: summed_transport_counts(stepper),
            replication: summed_replication_counts(stepper),
        }
    }
}

fn spawn(stepper: &mut Stepper<ReplicationStack>) -> StepperCounts {
    let before = StepperCounts::read(stepper);
    let entities = vec![(CompA(0.0), Replicate::to_clients(NetworkTarget::All)); ENTITIES];
    stepper.server_world_mut().spawn_batch(entities);
    stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);
    StepperCounts::read(stepper) - before
}

/// The frame's received bytes hold the client's protocol check too: 3 registries of a length
/// byte, a name of 16, 18 and 16 bytes and a hash of 32, 149 bytes, in a message of 155.
#[test]
fn a_frame_that_spawns_1000_entities_sends_exact_bytes() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    assert_eq!(
        spawn(&mut stepper),
        StepperCounts {
            transport: TransportCounts {
                sent_bytes: 8130,
                sent_packets: 9,
                resent_messages: 0,
                received_bytes: 8306,
            },
            replication: ChannelCounts {
                sent_bytes: 8018,
                sent_messages: 8,
                received_bytes: 8029,
            },
        }
    );
}

#[test]
fn a_frame_that_changes_1000_entities_sends_exact_bytes() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    spawn(&mut stepper);
    stepper.frame_step(1);
    let before = StepperCounts::read(&mut stepper);
    let world = stepper.server_world_mut();
    for mut component in world
        .query_filtered::<&mut CompA, With<Replicate>>()
        .iter_mut(world)
    {
        component.0 = 1.0;
    }
    stepper.step(1, stepper.frame_interval(), StepOrder::ServerFirst);
    assert_eq!(
        StepperCounts::read(&mut stepper) - before,
        StepperCounts {
            transport: TransportCounts {
                sent_bytes: 8150,
                sent_packets: 8,
                resent_messages: 0,
                received_bytes: 8143,
            },
            replication: ChannelCounts {
                sent_bytes: 8077,
                sent_messages: 7,
                received_bytes: 8088,
            },
        }
    );
}

/// A client that both targets name receives `Predicted` alone, and `Interpolated` in its place
/// once the server removes the prediction target. The spawn logs the overlap once for each
/// target it inserts, as each target's observer checks the other.
#[test]
fn a_client_in_both_targets_is_predicted_alone() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    let source = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
            InterpolationTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    let overlap = TargetsOverlap {
        entity: source.to_bits(),
        link: stepper.client_of_entity(0).to_bits(),
    };
    assert_eq!(stepper.log().take::<TargetsOverlap>(), [overlap, overlap]);
    stepper.frame_step(PROPAGATION_FRAMES);
    let markers = |stepper: &Stepper<ReplicationStack>| {
        let copy = stepper
            .client_copy(0, source)
            .expect("the entity reaches the client");
        let copy = stepper.client_world(0).entity(copy);
        [
            copy.contains::<Predicted>(),
            copy.contains::<Interpolated>(),
        ]
    };
    assert_eq!(markers(&stepper), [true, false]);

    {
        let world = stepper.server_world_mut();
        world.entity_mut(source).remove::<PredictionTarget>();
    };
    stepper.frame_step(PROPAGATION_FRAMES);
    assert_eq!(markers(&stepper), [false, true]);
}
