use super::*;
use crate::fixtures::replication_stack::ReplicationStack;
use campfire_net_connection::RemoteId;
use campfire_net_core::PeerId;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::{Init, StepperSetup};

#[derive(Debug, Resource, Default)]
struct Received(usize);

fn count(_: On<'_, '_, RemoteEvent<SenderHello>>, mut received: ResMut<'_, Received>) {
    received.0 += 1;
}

/// A peer can hold a dying link and its new one at once: the hello maps the sender's link
/// on the link it arrived on, the new one, and not on the dying one spawned first.
#[test]
fn a_hello_maps_on_the_link_it_arrived_on() {
    let mut world = World::new();
    world.add_observer(HelloPlugin::receive_sender_hello);
    let peer = PeerId::Entity(1);
    let dying = world.spawn((LinkMessages::default(), RemoteId(peer))).id();
    let new = world.spawn((LinkMessages::default(), RemoteId(peer))).id();
    let remote = Entity::from_raw_u32(7).unwrap();

    world.trigger(RemoteEvent {
        event: SenderHello {
            send_interval: PositiveTickSpan::from_ticks(1),
            link: remote,
        },
        from: peer,
        link: new,
    });

    let local = |link| {
        world
            .get::<LinkMessages>(link)
            .unwrap()
            .entity_mapper
            .remote_to_local()
            .get(&remote)
            .copied()
    };
    assert_eq!(local(new), Some(new));
    assert_eq!(local(dying), None);
}

/// Each end of a link that connects and replicates sends its hello once.
#[test]
fn a_replicating_link_sends_its_hello_once() {
    let mut setup = StepperSetup::single();
    setup.init = Init::Deferred;
    let mut stepper = Stepper::new(ReplicationStack, setup);
    stepper.client_app_mut(0).init_resource::<Received>();
    stepper.client_app_mut(0).add_observer(count);
    stepper.init();
    stepper.frame_step(10);

    assert_eq!(stepper.client_world(0).resource::<Received>().0, 1);
}
