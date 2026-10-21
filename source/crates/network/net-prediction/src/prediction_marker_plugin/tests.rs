use crate::fixtures::prediction_stack::PredictionStack;
use bevy_ecs::entity::Entity;
use bevy_ecs::hierarchy::ChildOf;
use bevy_ecs::lifecycle::Add;
use bevy_ecs::observer::On;
use bevy_ecs::query::With;
use bevy_ecs::system::Commands;
use bevy_ecs::system::Query;
use bevy_replicon::prelude::Remote;
use campfire_net_connection::RemoteId;
use campfire_net_connection::{ClientOf, Connected, NetworkTarget};
use campfire_net_core::PeerId;
use campfire_net_interpolation::internals::*;
use campfire_net_replication::internals::*;
use campfire_net_replication::{
    ControlledBy, ControlledLifetime, Interpolated, InterpolationTarget, Overridden, Predicted,
    PredictedSend, PredictionTarget, Replicate, RoomAllocator, Rooms,
};
use campfire_net_stepper::{Init, StepperSetup};
use campfire_net_stepper::{StepOrder, Stepper};

#[test]
fn late_join_client_gets_predicted_marker_for_prediction_target_all() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
            CompA(1.0),
        ))
        .id();

    let server_entity_ref = stepper.server_world().entity(server_entity);
    assert!(server_entity_ref.contains::<PredictedSend>());
    assert!(
        !server_entity_ref.contains::<Predicted>(),
        "a prediction target must not mark the authoritative sender as predicted"
    );

    stepper.frame_step(3);

    let client_0_entity = stepper.client_copy(0, server_entity).unwrap();
    assert!(
        stepper
            .client_world(0)
            .get::<Predicted>(client_0_entity)
            .is_some(),
        "existing client should see the entity as predicted"
    );
    assert!(
        stepper
            .client_world(0)
            .get::<PredictedSend>(client_0_entity)
            .is_none(),
        "the sender-only prediction marker must not be materialized on the receiver"
    );

    stepper.add_remote_client();
    stepper.init();
    stepper.frame_step(3);

    let client_1_entity = stepper
        .client_copy(1, server_entity)
        .expect("late-joining client should receive the entity");
    assert!(
        stepper
            .client_world(1)
            .get::<Predicted>(client_1_entity)
            .is_some(),
        "late-joining client should see PredictionTarget::All entity as predicted"
    );
}

/// How a test spawns the entity each of two clients predicts.
#[derive(Debug, Clone, Copy)]
enum Spawns {
    /// Both in one frame.
    Together,
    /// The second once the first replicated.
    Sequential,
    /// Both in one frame, each controlled by its client's link.
    ControlledTogether,
    /// As `Sequential`, each controlled by its client's link.
    Controlled,
    /// The first with one client connected, the second once a second client joined, each
    /// controlled by its client's link.
    LateJoin,
    /// By the server's observer of each client's connection, the second client joining late.
    Observer,
}

/// Whether a spawned entity is controlled by its client's link.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Control {
    Free,
    ByLink,
}

/// What a client holds of a server's entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    Predicted,
    Interpolated,
    Both,
    Neither,
    Missing,
}

/// Two clients' stepper, and the server's entity each predicts, client 0's first.
struct Scene {
    stepper: Stepper<PredictionStack>,
    entities: [Entity; 2],
}

impl Scene {
    fn new(spawns: Spawns) -> Self {
        match spawns {
            Spawns::Together
            | Spawns::Sequential
            | Spawns::ControlledTogether
            | Spawns::Controlled => {
                let mut stepper =
                    Stepper::new(PredictionStack, StepperSetup::with_plain_clients(2));
                let control = match spawns {
                    Spawns::ControlledTogether | Spawns::Controlled => Control::ByLink,
                    _ => Control::Free,
                };
                let sequential = matches!(spawns, Spawns::Sequential | Spawns::Controlled);
                let first = Self::spawn_for(&mut stepper, 0, control);
                if sequential {
                    stepper.frame_step(3);
                }
                let second = Self::spawn_for(&mut stepper, 1, control);
                stepper.frame_step(if sequential { 3 } else { 4 });
                Self {
                    stepper,
                    entities: [first, second],
                }
            }
            Spawns::LateJoin => {
                let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
                let first = Self::spawn_for(&mut stepper, 0, Control::ByLink);
                stepper.frame_step(3);
                stepper.add_remote_client();
                stepper.init();
                let second = Self::spawn_for(&mut stepper, 1, Control::ByLink);
                stepper.frame_step(3);
                Self {
                    stepper,
                    entities: [first, second],
                }
            }
            Spawns::Observer => {
                let mut setup = StepperSetup::single();
                setup.init = Init::Deferred;
                let mut stepper = Stepper::new(PredictionStack, setup);
                stepper.server_app_mut().add_observer(
                    |trigger: On<'_, '_, Add<Connected>>,
                     query: Query<'_, '_, &RemoteId, With<ClientOf>>,
                     mut commands: Commands<'_, '_>| {
                        if let Ok(remote_id) = query.get(trigger.entity) {
                            commands.spawn((
                                Self::targets(remote_id.0),
                                ControlledBy {
                                    link: trigger.entity,
                                    lifetime: ControlledLifetime::default(),
                                },
                            ));
                        }
                    },
                );
                stepper.init();
                stepper.frame_step(3);
                stepper.add_remote_client();
                stepper.init();
                stepper.frame_step(3);
                let entities = [0, 1].map(|client| {
                    let link = stepper.client_of_entity(client);
                    let world = stepper.server_world_mut();
                    let mut controlled = world.query::<(Entity, &ControlledBy)>();
                    let mut of_link = controlled
                        .iter(world)
                        .filter(|(_, controlled)| controlled.link == link);
                    let (entity, _) = of_link
                        .next()
                        .expect("the observer spawned the client's entity");
                    entity
                });
                Self { stepper, entities }
            }
        }
    }

    /// Replicated to all, predicted by the client of `id` alone, interpolated by every other.
    fn targets(id: PeerId) -> (Replicate, PredictionTarget, InterpolationTarget) {
        (
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(id)),
            InterpolationTarget::to_clients(NetworkTarget::all_except([id])),
        )
    }

    /// Spawns on the server the entity client `client` predicts.
    fn spawn_for(
        stepper: &mut Stepper<PredictionStack>,
        client: usize,
        control: Control,
    ) -> Entity {
        let id = stepper.client_of(client).get::<RemoteId>().unwrap().0;
        let link = stepper.client_of_entity(client);
        let world = stepper.server_world_mut();
        let mut entity = world.spawn(Self::targets(id));
        if control == Control::ByLink {
            entity.insert(ControlledBy {
                link,
                lifetime: ControlledLifetime::default(),
            });
        }
        entity.id()
    }

    /// What client `client` holds of the server's `entity`.
    fn held(&self, client: usize, entity: Entity) -> Held {
        let Some(copy) = self.stepper.client_copy(client, entity) else {
            return Held::Missing;
        };
        let world = self.stepper.client_world(client);
        match (
            world.get::<Predicted>(copy).is_some(),
            world.get::<Interpolated>(copy).is_some(),
        ) {
            (true, true) => Held::Both,
            (true, false) => Held::Predicted,
            (false, true) => Held::Interpolated,
            (false, false) => Held::Neither,
        }
    }

    /// How many entities client `client` predicts.
    fn predicted(&mut self, client: usize) -> usize {
        let world = self.stepper.client_world_mut(client);
        world
            .query_filtered::<Entity, With<Predicted>>()
            .iter(world)
            .count()
    }
}

#[test]
fn each_client_predicts_its_own_entity_and_interpolates_the_others_however_they_spawn() {
    use Held::{Interpolated as I, Predicted as P};
    let spawns = [
        Spawns::Together,
        Spawns::Sequential,
        Spawns::ControlledTogether,
        Spawns::Controlled,
        Spawns::LateJoin,
        Spawns::Observer,
    ];
    for spawns in spawns {
        let mut scene = Scene::new(spawns);
        let entities = scene.entities;
        let held = [0, 1].map(|client| entities.map(|entity| scene.held(client, entity)));
        assert_eq!(held, [[P, I], [I, P]], "{spawns:?}");
        assert_eq!(
            [0, 1].map(|client| scene.predicted(client)),
            [1, 1],
            "{spawns:?}"
        );
    }
}

#[test]
fn the_removal_of_a_prediction_target_removes_the_receiver_marker() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    let client_entity = stepper.client_copy(0, server_entity).unwrap();
    assert!(
        stepper
            .client_world(0)
            .entity(client_entity)
            .contains::<Predicted>()
    );

    stepper
        .server_world_mut()
        .entity_mut(server_entity)
        .remove::<PredictionTarget>();
    stepper.frame_step(PROPAGATION_FRAMES);

    assert!(
        !stepper
            .client_world(0)
            .entity(client_entity)
            .contains::<Predicted>()
    );
}

#[test]
fn a_client_that_joins_late_gets_the_latest_state_of_a_predicted_entity() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_entity = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
            CompA(0.0),
        ))
        .id();

    stepper.frame_step(PROPAGATION_FRAMES);

    for value in [1.0, 2.0, 3.0, 4.0] {
        stepper
            .server_world_mut()
            .entity_mut(server_entity)
            .insert(CompA(value));
        stepper.frame_step(1);
    }

    stepper.add_remote_client();
    stepper.init();
    stepper.frame_step(3);

    let client_1_entity = stepper
        .client_copy(1, server_entity)
        .expect("late-joining client should receive the entity");
    assert!(
        stepper
            .client_world(1)
            .get::<Predicted>(client_1_entity)
            .is_some(),
        "late-joining client should see the entity as predicted"
    );
    assert_eq!(
        stepper.client_world(1).get::<CompA>(client_1_entity),
        Some(&CompA(4.0)),
        "late-joining client should receive the latest component state"
    );
}

/// Re-inserting a Replicate component works as expected (doesn't
/// create duplicate entities)
/// https://github.com/cBournhonesque/lightyear/issues/1025
#[test]
fn reinserting_replicate_creates_no_duplicate_entity() {
    {
        let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

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
            world
                .entity_mut(source_entity)
                .insert(Replicate::to_clients(NetworkTarget::All));
        };
        stepper.frame_step(PROPAGATION_FRAMES);

        let mapped_after_reinsert = stepper
            .client_copy(0, source_entity)
            .unwrap_or_else(|| panic!("entity is not present in entity map"));
        assert_eq!(
            mapped_after_reinsert, mirrored_entity,
            "re-inserting Replicate should keep the same mapped entity"
        );

        let replicated_count = {
            let world = stepper.client_world_mut(0);
            world
                .query_filtered::<Entity, (With<CompA>, With<Remote>)>()
                .iter(world)
                .count()
        };
        assert_eq!(
            replicated_count, 1,
            "re-inserting Replicate should not create duplicate entities"
        );
    }
}

/// A child with explicitly set rooms keeps them (override):
/// plain children follow the root, but an overridden child replicates
/// independently and survives root room changes. Removing the override
/// follows the root's rooms again.
#[test]
fn a_child_with_its_own_rooms_overrides_the_root() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::with_plain_clients(2));

    let room_a = stepper
        .server_world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();
    let room_b = stepper
        .server_world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();
    let room_c = stepper
        .server_world_mut()
        .resource_mut::<RoomAllocator>()
        .allocate();

    // client 0 is in room A, client 1 is in room B, nobody is in room C
    let client_of_0_entity = stepper.client_of_entity(0);
    stepper
        .server_world_mut()
        .entity_mut(client_of_0_entity)
        .insert(Rooms::single(room_a));
    let client_of_1_entity = stepper.client_of_entity(1);
    stepper
        .server_world_mut()
        .entity_mut(client_of_1_entity)
        .insert(Rooms::single(room_b));

    let server_parent = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            CompFull(1.0),
            Rooms::single(room_a),
        ))
        .id();
    // plain child: rooms are followed on attach
    let server_child = stepper
        .server_world_mut()
        .spawn((ChildOf(server_parent), CompFull(2.0)))
        .id();
    // child spawned with different rooms: kept as an explicit override
    let server_other_child = stepper
        .server_world_mut()
        .spawn((ChildOf(server_parent), CompFull(3.0), Rooms::single(room_b)))
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    assert_eq!(
        stepper.server_world().get::<Rooms>(server_child),
        Some(&Rooms::single(room_a)),
        "plain child should follow the root's rooms"
    );
    assert!(
        stepper
            .server_world()
            .get::<Overridden<Rooms>>(server_child)
            .is_none(),
        "plain child should not be marked as overridden"
    );
    assert_eq!(
        stepper.server_world().get::<Rooms>(server_other_child),
        Some(&Rooms::single(room_b)),
        "explicit child rooms should be kept"
    );
    assert!(
        stepper
            .server_world()
            .get::<Overridden<Rooms>>(server_other_child)
            .is_some(),
        "explicit child rooms should be marked as overridden"
    );

    let is_visible = |stepper: &Stepper<PredictionStack>, client: usize, entity: Entity| {
        stepper.client_copy(client, entity).is_some()
    };
    // parent and plain child follow room A; the override follows room B
    assert!(is_visible(&stepper, 0, server_parent));
    assert!(is_visible(&stepper, 0, server_child));
    assert!(!is_visible(&stepper, 0, server_other_child));
    assert!(!is_visible(&stepper, 1, server_parent));
    assert!(!is_visible(&stepper, 1, server_child));
    assert!(is_visible(&stepper, 1, server_other_child));

    // move the parent to room C: the plain child follows, the override survives
    stepper
        .server_world_mut()
        .entity_mut(server_parent)
        .insert(Rooms::single(room_c));
    stepper.frame_step(PROPAGATION_FRAMES);

    assert_eq!(
        stepper.server_world().get::<Rooms>(server_child),
        Some(&Rooms::single(room_c)),
        "plain child should follow the root's new rooms"
    );
    assert_eq!(
        stepper.server_world().get::<Rooms>(server_other_child),
        Some(&Rooms::single(room_b)),
        "override should survive the root's room change"
    );
    assert!(!is_visible(&stepper, 0, server_parent));
    assert!(!is_visible(&stepper, 0, server_child));
    assert!(!is_visible(&stepper, 1, server_parent));
    assert!(!is_visible(&stepper, 1, server_child));
    assert!(
        is_visible(&stepper, 1, server_other_child),
        "override child should stay visible in room B"
    );

    // removing the override follows the root's rooms again
    stepper
        .server_world_mut()
        .entity_mut(server_other_child)
        .remove::<Overridden<Rooms>>();
    stepper.frame_step(PROPAGATION_FRAMES);

    assert_eq!(
        stepper.server_world().get::<Rooms>(server_other_child),
        Some(&Rooms::single(room_c)),
        "unmarked child should follow the root's rooms again"
    );
    assert!(
        !is_visible(&stepper, 1, server_other_child),
        "child should follow room C again"
    );
}

/// Switching the root's `PredictionTarget` reaches the hierarchy members:
/// the child follows the new target server-side and gains/loses `Predicted`
/// on the client.
#[test]
fn a_child_follows_a_root_prediction_target_switch() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());
    let client_id = stepper.client_of(0).get::<RemoteId>().unwrap().0;

    let server_root = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::single(client_id)),
        ))
        .id();
    let server_child = stepper.server_world_mut().spawn(ChildOf(server_root)).id();
    stepper.step(4, stepper.frame_interval(), StepOrder::ServerFirst);
    let client_child = stepper
        .client_copy(0, server_child)
        .expect("child should be replicated");

    // the child cloned the root's target and the client predicts it
    assert_eq!(
        stepper.server_world().get::<PredictionTarget>(server_child),
        stepper.server_world().get::<PredictionTarget>(server_root),
    );
    assert!(
        stepper
            .client_world(0)
            .get::<Predicted>(client_child)
            .is_some(),
        "targeted client should predict the child"
    );

    // switch the root target away from the client, in place
    stepper
        .server_world_mut()
        .entity_mut(server_root)
        .insert(PredictionTarget::to_clients(NetworkTarget::all_except([
            client_id,
        ])));
    stepper.step(6, stepper.frame_interval(), StepOrder::ServerFirst);

    // the child followed the switch server-side...
    assert_eq!(
        stepper.server_world().get::<PredictionTarget>(server_child),
        stepper.server_world().get::<PredictionTarget>(server_root),
    );
    // ...and the client stopped predicting it
    assert!(
        stepper
            .client_world(0)
            .get::<Predicted>(client_child)
            .is_none(),
        "untargeted client should no longer predict the child"
    );
}

/// https://github.com/cBournhonesque/lightyear/issues/627
/// A predicted parent and its child keep their hierarchy on the client: the child's `ChildOf`
/// names the client's entity for the server's parent.
#[test]
fn a_predicted_hierarchy_keeps_its_child_of_on_the_client() {
    let mut stepper = Stepper::new(PredictionStack, StepperSetup::single());

    let server_child = stepper.server_world_mut().spawn_empty().id();
    let server_parent = stepper
        .server_world_mut()
        .spawn((
            Replicate::to_clients(NetworkTarget::All),
            PredictionTarget::to_clients(NetworkTarget::All),
        ))
        .add_child(server_child)
        .id();
    stepper.frame_step(PROPAGATION_FRAMES);

    // check that the parent and child are spawned on the client
    let predicted_child = stepper
        .client_copy(0, server_child)
        .expect("child entity was not replicated to client");
    let predicted_parent = stepper
        .client_copy(0, server_parent)
        .expect("parent entity was not replicated to client");

    // check that the parent-child hierarchy is maintained
    assert_eq!(
        stepper
            .client_world(0)
            .get::<ChildOf>(predicted_child)
            .expect("predicted child entity doesn't have a parent")
            .parent(),
        predicted_parent
    );
}
