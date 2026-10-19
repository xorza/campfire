use super::*;
use crate::disable_replicate_hierarchy::DisableReplicateHierarchy;
use crate::fixtures::protocol::*;
use crate::fixtures::replication_stack::ReplicationStack;
use crate::fixtures::steps::client_copy;
use crate::manual_visibility::ManualVisibility;
use crate::overridden::Overridden;
use crate::replicate_like::ReplicateLike;
use crate::replication_target::Replicate;
use crate::room_allocator::RoomAllocator;
use crate::room_id::RoomId;
use crate::rooms::Rooms;
use crate::visibility_ext::VisibilityExt;
use bevy_replicon::shared::server_entity_map::ServerEntityMap;
use campfire_net_connection::NetworkTarget;
use campfire_net_connection::RemoteId;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;

use crate::hierarchy_send_plugin::HierarchySendPlugin;
use crate::inheritance_plugin::InheritancePlugin;

use bevy_app::App;

use bevy_replicon::prelude::{AuthMethod, RepliconSharedPlugin};
use bevy_replicon::server::ServerPlugin;
use bevy_state::app::StatesPlugin;

fn app_with_hierarchy_plugin() -> App {
    let mut app = App::default();
    app.add_plugins(StatesPlugin);
    app.init_resource::<bevy_time::Time>();
    app.add_plugins(RepliconSharedPlugin {
        auth_method: AuthMethod::None,
    });
    app.add_plugins(ServerPlugin::default());
    app.add_plugins(HierarchySendPlugin);
    app.add_plugins(InheritancePlugin);
    app
}

/// A change to the `Children` of an entity with `ReplicationMarker` attaches every child to its
/// root.
#[test]
fn a_children_change_attaches_every_child_to_its_root() {
    let mut app = app_with_hierarchy_plugin();

    let grandparent = app.world_mut().spawn(Replicate::to_links([])).id();
    // parent with no ReplicationMarker: ReplicateLike should be added
    let grandchild_1 = app.world_mut().spawn_empty().id();
    let child_1 = app.world_mut().spawn_empty().id();
    let parent_1 = app.world_mut().spawn_empty().add_child(child_1).id();

    // parent with ReplicationMarker: it is attached like any other child, and
    // its whole subtree inherits from the ultimate root (flattening)
    let child_2a = app.world_mut().spawn_empty().id();
    let child_2b = app.world_mut().spawn(Replicate::default()).id();
    let grandchild_2b = app.world_mut().spawn_empty().id();
    let child_2c = app
        .world_mut()
        .spawn(ReplicateLike { root: grandparent })
        .id();
    let parent_2 = app
        .world_mut()
        .spawn(Replicate::default())
        .add_children(&[child_2a, child_2b, child_2c])
        .id();

    // parent has Replicate::to_links([]) and DisableReplicate so ReplicateLike is not added
    let child_3a = app.world_mut().spawn_empty().id();
    let child_3b = app
        .world_mut()
        .spawn(ReplicateLike { root: grandparent })
        .id();
    let parent_3 = app
        .world_mut()
        .spawn((Replicate::to_links([]), DisableReplicateHierarchy))
        .add_children(&[child_3a, child_3b])
        .id();

    // parent has DisableReplicate so ReplicateLike is not added
    let child_4 = app.world_mut().spawn_empty().id();
    let parent_4 = app
        .world_mut()
        .spawn(DisableReplicateHierarchy)
        .add_child(child_4)
        .id();

    // add Children to the entity which already has Replicate::to_links([])
    app.world_mut()
        .entity_mut(grandparent)
        .add_children(&[parent_1, parent_2, parent_3, parent_4]);

    // flush commands
    app.update();

    // Add grandchildren which should also get ReplicateLike
    app.world_mut().entity_mut(parent_1).add_child(grandchild_1);
    app.world_mut()
        .entity_mut(child_2b)
        .add_child(grandchild_2b);

    app.update();

    assert_eq!(
        app.world().get::<ReplicateLike>(parent_1).unwrap().root,
        grandparent
    );
    assert_eq!(
        app.world().get::<ReplicateLike>(child_1).unwrap().root,
        grandparent
    );

    // parent_2 is attached despite its own `Replicate`, which it keeps as an override
    assert_eq!(
        app.world().get::<ReplicateLike>(parent_2).unwrap().root,
        grandparent
    );
    assert!(app.world().get::<Replicate>(parent_2).is_some());
    assert!(app.world().get::<Overridden<Replicate>>(parent_2).is_some());
    // the whole subtree inherits from the ultimate root (flattening):
    // child_2a follows the grandparent's `Replicate`, child_2b keeps its own
    assert_eq!(
        app.world().get::<ReplicateLike>(child_2a).unwrap().root,
        grandparent
    );
    assert_eq!(
        app.world().get::<Replicate>(child_2a),
        Some(&Replicate::to_links([]))
    );
    assert!(app.world().get::<Overridden<Replicate>>(child_2a).is_none());
    assert_eq!(
        app.world().get::<ReplicateLike>(child_2b).unwrap().root,
        grandparent
    );
    assert!(app.world().get::<Replicate>(child_2b).is_some());
    assert!(app.world().get::<Overridden<Replicate>>(child_2b).is_some());
    // manually added attachments are preserved, not overwritten
    assert_eq!(
        app.world().get::<ReplicateLike>(child_2c).unwrap().root,
        grandparent
    );

    assert!(app.world().get::<ReplicateLike>(parent_3).is_none());
    assert!(app.world().get::<ReplicateLike>(child_3a).is_none());
    // the parent had DisableReplicateHierarchy so the existing ReplicateLike is not overwritten
    assert_eq!(
        app.world().get::<ReplicateLike>(child_3b).unwrap().root,
        grandparent
    );

    // DisableReplicateHierarchy means that ReplicateLike is not added and is not added
    // on the entity itself either
    assert!(app.world().get::<ReplicateLike>(parent_4).is_none());
    assert!(app.world().get::<ReplicateLike>(child_4).is_none());

    // The grandchild should replicate like its parent -> grandparent
    assert_eq!(
        app.world().get::<ReplicateLike>(grandchild_1).unwrap().root,
        grandparent
    );
    // ...even below a child with its own `Replicate`: overrides are
    // per-entity, inheritance always comes from the ultimate root
    assert_eq!(
        app.world()
            .get::<ReplicateLike>(grandchild_2b)
            .unwrap()
            .root,
        grandparent
    );
}

/// A `ReplicationMarker` added to an entity that already has children attaches them to their root.
#[test]
fn a_replication_marker_attaches_the_children_below_it() {
    let mut app = app_with_hierarchy_plugin();

    let grandparent = app.world_mut().spawn_empty().id();
    // parent with no ReplicationMarker: ReplicateLike should be added
    let child_1 = app.world_mut().spawn_empty().id();
    let parent_1 = app.world_mut().spawn_empty().add_child(child_1).id();

    // parent with ReplicationMarker: the root ReplicateLike shouldn't be added
    // but the intermediary ReplicateLike should be added to child 2a
    let child_2a = app.world_mut().spawn_empty().id();
    let child_2b = app.world_mut().spawn(Replicate::default()).id();
    let child_2c = app
        .world_mut()
        .spawn(ReplicateLike { root: grandparent })
        .id();
    let parent_2 = app
        .world_mut()
        .spawn(Replicate::default())
        .add_children(&[child_2a, child_2b, child_2c])
        .id();

    // parent has ReplicationMarker and DisableReplicate so ReplicateLike is not added
    let child_3a = app.world_mut().spawn_empty().id();
    let child_3b = app
        .world_mut()
        .spawn(ReplicateLike { root: grandparent })
        .id();
    let parent_3 = app
        .world_mut()
        .spawn((Replicate::to_links([]), DisableReplicateHierarchy))
        .add_children(&[child_3a, child_3b])
        .id();

    // parent has DisableReplicate so ReplicateLike is not added
    let child_4 = app.world_mut().spawn_empty().id();
    let parent_4 = app
        .world_mut()
        .spawn(DisableReplicateHierarchy)
        .add_child(child_4)
        .id();

    app.world_mut()
        .entity_mut(grandparent)
        .add_children(&[parent_1, parent_2, parent_3, parent_4]);
    // add ReplicationMarker to an entity that already has children
    app.world_mut()
        .entity_mut(grandparent)
        .insert(Replicate::to_links([]));

    // flush commands
    app.update();
    assert_eq!(
        app.world().get::<ReplicateLike>(parent_1).unwrap().root,
        grandparent
    );
    assert_eq!(
        app.world().get::<ReplicateLike>(child_1).unwrap().root,
        grandparent
    );

    // parent_2 is attached despite its own `Replicate`, which it keeps as an override
    assert_eq!(
        app.world().get::<ReplicateLike>(parent_2).unwrap().root,
        grandparent
    );
    assert!(app.world().get::<Replicate>(parent_2).is_some());
    assert!(app.world().get::<Overridden<Replicate>>(parent_2).is_some());
    assert_eq!(
        app.world().get::<ReplicateLike>(child_2a).unwrap().root,
        grandparent
    );
    assert_eq!(
        app.world().get::<ReplicateLike>(child_2b).unwrap().root,
        grandparent
    );
    assert!(app.world().get::<Replicate>(child_2b).is_some());
    assert!(app.world().get::<Overridden<Replicate>>(child_2b).is_some());
    // manually added attachments are preserved, not overwritten
    assert_eq!(
        app.world().get::<ReplicateLike>(child_2c).unwrap().root,
        grandparent
    );

    assert!(app.world().get::<ReplicateLike>(parent_3).is_none());
    assert!(app.world().get::<ReplicateLike>(child_3a).is_none());
    // the parent had DisableReplicateHierarchy so the existing ReplicateLike is not overwritten
    assert_eq!(
        app.world().get::<ReplicateLike>(child_3b).unwrap().root,
        grandparent
    );

    // DisableReplicateHierarchy means that ReplicateLike is not added and is not added
    // on the entity itself either
    assert!(app.world().get::<ReplicateLike>(parent_4).is_none());
    assert!(app.world().get::<ReplicateLike>(child_4).is_none());
}

/// A root that stops replicating takes its members' copies of its `Replicate` with it, and
/// their attachments; a member's own `Replicate` stays.
#[test]
fn a_root_that_stops_replicating_clears_its_members_copies() {
    let mut app = app_with_hierarchy_plugin();
    let cloned = app.world_mut().spawn_empty().id();
    let own = app.world_mut().spawn(Replicate::default()).id();
    let root = app
        .world_mut()
        .spawn(Replicate::to_links([]))
        .add_children(&[cloned, own])
        .id();
    app.update();
    assert!(app.world().entity(cloned).contains::<Replicate>());
    assert!(
        !app.world()
            .entity(cloned)
            .contains::<Overridden<Replicate>>()
    );

    app.world_mut().entity_mut(root).remove::<Replicate>();
    app.update();

    let cloned = app.world().entity(cloned);
    assert!(!cloned.contains::<Replicate>());
    assert!(!cloned.contains::<ReplicateLike>());
    let own = app.world().entity(own);
    assert!(own.contains::<Replicate>());
    assert!(!own.contains::<ReplicateLike>());
}

/// A child that leaves its parent leaves its root: a member with only a copy of the root's
/// `Replicate` loses it and its `ReplicateLike`, and a member with a `Replicate` of its own becomes
/// the root of its subtree.
#[test]
fn an_unparented_child_leaves_its_root() {
    let mut app = app_with_hierarchy_plugin();
    let grandchild = app.world_mut().spawn_empty().id();
    let own = app
        .world_mut()
        .spawn(Replicate::default())
        .add_child(grandchild)
        .id();
    let cloned_grandchild = app.world_mut().spawn_empty().id();
    let cloned = app
        .world_mut()
        .spawn_empty()
        .add_child(cloned_grandchild)
        .id();
    let root = app
        .world_mut()
        .spawn(Replicate::to_links([]))
        .add_children(&[own, cloned])
        .id();
    app.update();
    assert_eq!(
        app.world().get::<ReplicateLike>(grandchild).unwrap().root,
        root
    );

    app.world_mut().entity_mut(own).remove::<ChildOf>();
    app.world_mut().entity_mut(cloned).remove::<ChildOf>();
    app.update();

    assert!(app.world().get::<ReplicateLike>(own).is_none());
    assert_eq!(
        app.world().get::<ReplicateLike>(grandchild).unwrap().root,
        own
    );
    for entity in [cloned, cloned_grandchild] {
        let entity = app.world().entity(entity);
        assert!(!entity.contains::<ReplicateLike>());
        assert!(!entity.contains::<Replicate>());
        assert!(!entity.contains::<Overridden<Replicate>>());
    }
}

/// `DisableReplicateHierarchy` added to an attached member detaches it and its subtree, as it does
/// for a member that has it when it joins.
#[test]
fn a_later_disable_detaches_the_subtree() {
    let mut app = app_with_hierarchy_plugin();
    let grandchild = app.world_mut().spawn_empty().id();
    let child = app.world_mut().spawn_empty().add_child(grandchild).id();
    app.world_mut()
        .spawn(Replicate::to_links([]))
        .add_child(child);
    app.update();
    assert!(app.world().get::<ReplicateLike>(grandchild).is_some());

    app.world_mut()
        .entity_mut(child)
        .insert(DisableReplicateHierarchy);
    app.update();

    for entity in [child, grandchild] {
        let entity = app.world().entity(entity);
        assert!(!entity.contains::<ReplicateLike>());
        assert!(!entity.contains::<Replicate>());
    }
}

/// A child moved to another parent follows the new root's aspects, and keeps the ones it
/// overrides.
#[test]
fn a_reparented_child_follows_its_new_root() {
    let mut app = app_with_hierarchy_plugin();
    let root_a = app
        .world_mut()
        .spawn((Replicate::to_links([]), Rooms::single(RoomId(0))))
        .id();
    let root_b = app
        .world_mut()
        .spawn((Replicate::default(), Rooms::single(RoomId(1))))
        .id();
    let plain = app.world_mut().spawn(ChildOf(root_a)).id();
    let dummy = app.world_mut().spawn_empty().id();
    let own = app
        .world_mut()
        .spawn((
            ChildOf(root_a),
            Replicate::to_links([dummy]),
            Rooms::single(RoomId(2)),
        ))
        .id();
    app.update();
    assert_eq!(
        app.world().get::<Rooms>(plain),
        Some(&Rooms::single(RoomId(0)))
    );

    app.world_mut().entity_mut(plain).insert(ChildOf(root_b));
    app.world_mut().entity_mut(own).insert(ChildOf(root_b));
    app.update();

    let plain = app.world().entity(plain);
    assert_eq!(plain.get::<ReplicateLike>().unwrap().root, root_b);
    assert_eq!(plain.get::<Replicate>(), Some(&Replicate::default()));
    assert_eq!(plain.get::<Rooms>(), Some(&Rooms::single(RoomId(1))));
    assert!(!plain.contains::<Overridden<Replicate>>());
    assert!(!plain.contains::<Overridden<Rooms>>());
    let own = app.world().entity(own);
    assert_eq!(own.get::<ReplicateLike>().unwrap().root, root_b);
    assert_eq!(own.get::<Replicate>(), Some(&Replicate::to_links([dummy])));
    assert_eq!(own.get::<Rooms>(), Some(&Rooms::single(RoomId(2))));
    assert!(own.contains::<Overridden<Replicate>>());
    assert!(own.contains::<Overridden<Rooms>>());
}

fn setup_hierarchy() -> (Stepper<ReplicationStack>, Entity, Entity, Entity) {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    let grandparent = stepper.server_app_mut().world_mut().spawn_empty().id();
    let parent = stepper
        .server_app_mut()
        .world_mut()
        .spawn(ChildOf(grandparent))
        .id();
    let child = stepper
        .server_app_mut()
        .world_mut()
        .spawn(ChildOf(parent))
        .id();
    (stepper, grandparent, parent, child)
}

/// Add a child to a replicated Entity: the child should be replicated
/// and the ChildOf component should be present on the replicated entity
#[test]
fn a_replicated_child_keeps_its_child_of() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());

    let server_entity = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All),))
        .id();
    stepper.frame_step(2);
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
        .spawn((ChildOf(server_entity),))
        .id();
    stepper.frame_step(2);
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
}

#[test]
fn an_attached_hierarchy_replicates_and_a_detached_member_does_not() {
    let (mut stepper, grandparent, parent, child) = setup_hierarchy();

    let replicate = Replicate::to_links([stepper.client_of_entity(0)]);
    // disable attaching for the child, so the child won't have ReplicateLike or RelationshipSync
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(child)
        .insert(DisableReplicateHierarchy);
    // add Replicate, which should add the RelationshipSync and ReplicateLike through the hierarchy
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(grandparent)
        .insert(replicate);
    stepper.frame_step(2);

    // check that the parent got replicated, along with the hierarchy information
    let client_grandparent = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&grandparent)
        .copied()
        .expect("entity is not present in entity map");
    let _client_parent = stepper
        .client_app(0)
        .world()
        .resource::<ServerEntityMap>()
        .to_client()
        .get(&parent)
        .copied()
        .expect("entity is not present in entity map");

    let (client_parent, client_parent_component) = stepper
        .client_app_mut(0)
        .world_mut()
        .query::<(Entity, &ChildOf)>()
        .single(stepper.client_app(0).world())
        .unwrap();

    assert_eq!(client_parent_component.parent(), client_grandparent);

    // TODO: check that the parent/grandparent have the same ReplicationGroupId

    // check that the child did not get replicated
    assert!(
        stepper
            .server_app()
            .world()
            .get::<ReplicateLike>(child)
            .is_none()
    );

    // The parent leaves the hierarchy: it had only a copy of the root's `Replicate`, so it stops
    // replicating, and the client despawns it.
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(parent)
        .remove::<ChildOf>();
    stepper.frame_step(2);

    assert!(
        stepper
            .client_app(0)
            .world()
            .get_entity(client_parent)
            .is_err()
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<Children>(client_grandparent)
            .is_none()
    );
}

/// Lose_visibility on a parent reaches ReplicateLike children.
#[test]
fn children_follow_a_parents_lost_visibility() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    let server_parent = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All), CompA(1.0)))
        .id();
    let server_child = stepper
        .server_app_mut()
        .world_mut()
        .spawn((ChildOf(server_parent), CompA(2.0)))
        .id();
    stepper.frame_step(2);

    // Both parent and child should be visible to both clients initially
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_parent)
            .copied()
            .is_some(),
        "client 1 should see parent"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_child)
            .copied()
            .is_some(),
        "client 1 should see child"
    );

    // Lose visibility for ONLY the parent on client 1
    let sender_1 = stepper.client_of_entity(1);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility(server_parent, sender_1);
    stepper.frame_step(2);

    // Parent should be hidden for client 1
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_parent)
            .copied()
            .is_none(),
        "client 1 should not see parent after lose_visibility"
    );

    // Child should also be hidden — lose_visibility reaches it through the ChildOf hierarchy
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_child)
            .copied()
            .is_none(),
        "child should not be visible after lose_visibility on parent"
    );
}

/// A child spawned AFTER lose_visibility on the parent inherits
/// the parent's hidden state instead of being visible by default.
#[test]
fn a_late_child_of_a_hidden_parent_stays_hidden() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    let server_parent = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All), CompA(1.0)))
        .id();
    stepper.frame_step(2);

    // Both clients see the parent initially
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_parent)
            .copied()
            .is_some(),
        "client 1 should see parent"
    );

    // Hide the parent from client 1 before the child exists
    let sender_1 = stepper.client_of_entity(1);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility(server_parent, sender_1);
    stepper.frame_step(2);
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_parent)
            .copied()
            .is_none(),
        "client 1 should not see parent after lose_visibility"
    );

    // Spawn the child after the parent was hidden
    let server_child = stepper
        .server_app_mut()
        .world_mut()
        .spawn((ChildOf(server_parent), CompA(2.0)))
        .id();
    stepper.frame_step(2);

    assert_eq!(
        stepper
            .server_app()
            .world()
            .get::<ReplicateLike>(server_child)
            .unwrap()
            .root,
        server_parent
    );
    // Client 0 (parent still visible) should see the late child
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_child)
            .copied()
            .is_some(),
        "client 0 should see late-spawned child"
    );
    // Client 1 (parent hidden) should NOT see the late child
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_child)
            .copied()
            .is_none(),
        "late-spawned child should inherit lose_visibility from parent"
    );
}

/// A child with explicitly set visibility keeps it (override):
/// direct writes on a member insert `Overridden<ManualVisibility>` and
/// later parent changes no longer reach it. Removing the marker
/// follows the parent again, and an overridden-visible child replicates while hidden.
#[test]
fn a_child_with_its_own_visibility_overrides_the_parent() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    let server_parent = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All), CompA(1.0)))
        .id();
    let server_child = stepper
        .server_app_mut()
        .world_mut()
        .spawn((ChildOf(server_parent), CompA(2.0)))
        .id();
    stepper.frame_step(2);

    let is_visible = |stepper: &Stepper<ReplicationStack>, entity: Entity| {
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&entity)
            .copied()
            .is_some()
    };
    let is_marked = |stepper: &Stepper<ReplicationStack>| {
        stepper
            .server_app()
            .world()
            .get::<Overridden<ManualVisibility>>(server_child)
            .is_some()
    };
    let sender_1 = stepper.client_of_entity(1);

    assert!(is_visible(&stepper, server_parent));
    assert!(is_visible(&stepper, server_child));

    // hide the child directly: it stays hidden across parent pushes
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility(server_child, sender_1);
    stepper.frame_step(2);
    assert!(is_visible(&stepper, server_parent));
    assert!(!is_visible(&stepper, server_child));
    assert!(is_marked(&stepper), "direct write should mark the child");

    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .gain_visibility(server_parent, sender_1);
    stepper.frame_step(2);
    assert!(is_visible(&stepper, server_parent));
    assert!(
        !is_visible(&stepper, server_child),
        "parent push should skip the overridden child"
    );

    // removing the marker follows the parent's (visible) state again
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_child)
        .remove::<Overridden<ManualVisibility>>();
    stepper.frame_step(2);
    assert!(is_visible(&stepper, server_child));
    assert!(!is_marked(&stepper));

    // hiding the parent hides the child again (it follows once more)
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility(server_parent, sender_1);
    stepper.frame_step(2);
    assert!(!is_visible(&stepper, server_parent));
    assert!(!is_visible(&stepper, server_child));

    // widening: the child replicates while the parent stays hidden
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .gain_visibility(server_child, sender_1);
    stepper.frame_step(2);
    assert!(!is_visible(&stepper, server_parent));
    assert!(
        is_visible(&stepper, server_child),
        "overridden child should replicate while the parent is hidden"
    );
    assert!(is_marked(&stepper));
}

/// A child with its own `Replicate` is still attached (`ReplicateLike`)
/// and inherits rooms/visibility, while its own `Replicate` acts as an
/// override: here the child's target includes a client the rooms exclude.
#[test]
fn a_child_with_its_own_replicate_follows_the_rooms() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    let room_a = stepper
        .server_app_mut()
        .world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();
    let room_b = stepper
        .server_app_mut()
        .world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();

    // client 0 is in room A, client 1 is in room B
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(client_of_0_entity)
        .insert(Rooms::single(room_a));
    let client_of_1_entity = stepper.client_of_entity(1);
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(client_of_1_entity)
        .insert(Rooms::single(room_b));

    let client_0_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;

    // parent replicates to client 0 only and lives in room A
    let server_parent = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::single(client_0_id)),
            CompA(1.0),
            Rooms::single(room_a),
        ))
        .id();
    // child overrides the target (all clients) but inherits rooms
    let server_child = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            ChildOf(server_parent),
            Replicate::to_clients(NetworkTarget::All),
            CompA(2.0),
        ))
        .id();
    stepper.frame_step(2);

    // attached, own `Replicate` kept, rooms followed from the parent
    assert_eq!(
        stepper
            .server_app()
            .world()
            .get::<ReplicateLike>(server_child)
            .map(|like| like.root),
        Some(server_parent)
    );
    assert_eq!(
        stepper.server_app().world().get::<Replicate>(server_child),
        Some(&Replicate::to_clients(NetworkTarget::All))
    );
    assert_eq!(
        stepper.server_app().world().get::<Rooms>(server_child),
        Some(&Rooms::single(room_a))
    );
    assert!(
        stepper
            .server_app()
            .world()
            .get::<Overridden<Rooms>>(server_child)
            .is_none()
    );

    let is_visible = |stepper: &Stepper<ReplicationStack>, client: usize, entity: Entity| {
        stepper
            .client_app(client)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&entity)
            .copied()
            .is_some()
    };
    // client 0 sees both; client 1 sees neither: the child's own target
    // includes it, but the inherited rooms exclude it
    assert!(is_visible(&stepper, 0, server_parent));
    assert!(is_visible(&stepper, 0, server_child));
    assert!(!is_visible(&stepper, 1, server_parent));
    assert!(
        !is_visible(&stepper, 1, server_child),
        "inherited rooms should win over the child's own target"
    );
}

/// A child with ReplicateLike follows the parent's room visibility,
/// even though the child itself has no Rooms component.
#[test]
fn children_follow_the_parents_rooms() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::with_plain_clients(2));

    let room_a = stepper
        .server_app_mut()
        .world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();
    let room_b = stepper
        .server_app_mut()
        .world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();

    // client 0 is in room A, client 1 is in room B
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(client_of_0_entity)
        .insert(Rooms::single(room_a));
    let client_of_1_entity = stepper.client_of_entity(1);
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(client_of_1_entity)
        .insert(Rooms::single(room_b));

    let server_parent = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            CompA(1.0),
            Rooms::single(room_a),
        ))
        .id();
    // the child has no Rooms of its own; it should use the root's rooms
    let server_child = stepper
        .server_app_mut()
        .world_mut()
        .spawn((ChildOf(server_parent), CompA(2.0)))
        .id();
    stepper.frame_step(2);

    // The hierarchy walk should have attached the child to the parent
    assert_eq!(
        stepper
            .server_app()
            .world()
            .get::<ReplicateLike>(server_child)
            .unwrap()
            .root,
        server_parent
    );

    // parent is in room A: only client 0 sees parent and child
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_parent)
            .copied()
            .is_some(),
        "client 0 should see parent in room A"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_child)
            .copied()
            .is_some(),
        "client 0 should see child of parent in room A"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_parent)
            .copied()
            .is_none(),
        "client 1 should not see parent in room A"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_child)
            .copied()
            .is_none(),
        "client 1 should not see child of parent in room A"
    );

    // move the parent to room B: visibility should flip for both parent and child
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(server_parent)
        .insert(Rooms::single(room_b));
    stepper.frame_step(2);

    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_parent)
            .copied()
            .is_none(),
        "client 0 should not see parent after it moved to room B"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_child)
            .copied()
            .is_none(),
        "client 0 should not see child after parent moved to room B"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_parent)
            .copied()
            .is_some(),
        "client 1 should see parent after it moved to room B"
    );
    assert!(
        stepper
            .client_app(1)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_child)
            .copied()
            .is_some(),
        "client 1 should see child after parent moved to room B"
    );

    // move client 0 into room B as well: it should see parent and child again
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(client_of_0_entity)
        .insert(Rooms::single(room_b));
    stepper.frame_step(2);

    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_parent)
            .copied()
            .is_some(),
        "client 0 should see parent after joining room B"
    );
    assert!(
        stepper
            .client_app(0)
            .world()
            .resource::<ServerEntityMap>()
            .to_client()
            .get(&server_child)
            .copied()
            .is_some(),
        "client 0 should see child after joining room B"
    );
}

/// A child that arrives before its parent waits for it, and attaches in the frame the parent's
/// mapping arrives.
#[test]
fn a_child_attaches_once_its_parent_arrives() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    let parent = stepper.server_app_mut().world_mut().spawn_empty().id();
    let child = stepper
        .server_app_mut()
        .world_mut()
        .spawn((Replicate::to_clients(NetworkTarget::All), ChildOf(parent)))
        .id();
    stepper.frame_step(2);
    let client_child = client_copy(&stepper, child).expect("the child replicates alone");
    assert!(
        stepper
            .client_app(0)
            .world()
            .get::<ChildOf>(client_child)
            .is_none()
    );

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(parent)
        .insert(Replicate::to_clients(NetworkTarget::All));
    stepper.frame_step(2);

    let client_parent = client_copy(&stepper, parent).expect("the parent replicates");
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<ChildOf>(client_child)
            .map(ChildOf::parent),
        Some(client_parent)
    );
}

/// A member with a `Replicate` of its own outlives a root that stops replicating: the client's
/// despawn of the root takes the root and a local child of it, and the member stays, with no
/// `ChildOf` and with its `RemoteParent`.
#[test]
fn a_member_outlives_a_root_that_stops_replicating() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    let client_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;
    let root = stepper
        .server_app_mut()
        .world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    let member = stepper
        .server_app_mut()
        .world_mut()
        .spawn((
            ChildOf(root),
            Replicate::to_clients(NetworkTarget::single(client_id)),
        ))
        .id();
    stepper.frame_step(2);
    let client_root = client_copy(&stepper, root).unwrap();
    let client_member = client_copy(&stepper, member).unwrap();
    assert_eq!(
        stepper
            .client_app(0)
            .world()
            .get::<ChildOf>(client_member)
            .map(ChildOf::parent),
        Some(client_root)
    );
    let local = stepper
        .client_app_mut(0)
        .world_mut()
        .spawn(ChildOf(client_root))
        .id();

    stepper
        .server_app_mut()
        .world_mut()
        .entity_mut(root)
        .remove::<Replicate>();
    stepper.frame_step(2);

    let world = stepper.client_app(0).world();
    assert!(world.get_entity(client_root).is_err());
    assert!(world.get_entity(local).is_err());
    let client_member = world.entity(client_member);
    assert!(!client_member.contains::<ChildOf>());
    assert!(client_member.contains::<RemoteParent>());
}

/// A member with a visibility of its own outlives its root's hiding, with no `ChildOf`, and gets
/// its `ChildOf` back to the root's new copy once the root is visible again.
#[test]
fn a_member_outlives_its_hidden_root_and_attaches_when_it_returns() {
    let mut stepper = Stepper::new(ReplicationStack, StepperSetup::single());
    let root = stepper
        .server_app_mut()
        .world_mut()
        .spawn(Replicate::to_clients(NetworkTarget::All))
        .id();
    let member = stepper
        .server_app_mut()
        .world_mut()
        .spawn((ChildOf(root), CompA(1.0)))
        .id();
    stepper.frame_step(2);
    let client_member = client_copy(&stepper, member).unwrap();
    let link = stepper.client_of_entity(0);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .gain_visibility(member, link);
    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .lose_visibility(root, link);
    stepper.frame_step(2);

    assert_eq!(client_copy(&stepper, root), None);
    let parent = |stepper: &Stepper<ReplicationStack>| {
        stepper
            .client_app(0)
            .world()
            .get::<ChildOf>(client_member)
            .map(ChildOf::parent)
    };
    assert_eq!(parent(&stepper), None);
    assert!(
        stepper
            .client_app(0)
            .world()
            .entity(client_member)
            .contains::<RemoteParent>()
    );

    stepper
        .server_app_mut()
        .world_mut()
        .commands()
        .gain_visibility(root, link);
    stepper.frame_step(2);
    let client_root = client_copy(&stepper, root).unwrap();
    assert_eq!(parent(&stepper), Some(client_root));
}
