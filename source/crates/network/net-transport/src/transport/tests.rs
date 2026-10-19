use super::*;
use crate::fixtures::protocol::{Channel1, channel_id};
use crate::fixtures::transport_stack::TransportStack;
use bevy_ecs::world::World;
use bytes::Bytes;
use campfire_net_channels::fragment_size_for_max_payload;
use campfire_net_channels::internals::ChannelCounts;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperConfig;

/// A 5-byte message counts 5 bytes sent on the client and 5 received on the server, and
/// nothing the other way.
#[test]
fn one_message_counts_on_both_sides() {
    let mut stepper = Stepper::new(TransportStack, StepperConfig::single());
    let channel = ChannelKind::of::<Channel1>();
    let read = |stepper: &Stepper<TransportStack>, client: bool| {
        let link = if client {
            stepper.client(0)
        } else {
            stepper.client_of(0)
        };
        link.get::<Transport>().unwrap().channel_counts(channel)
    };
    let (client_before, server_before) = (read(&stepper, true), read(&stepper, false));

    let channel_id = channel_id::<Channel1>(stepper.client_app(0));
    stepper
        .client_mut(0)
        .get_mut::<Transport>()
        .unwrap()
        .send(channel_id, Bytes::from_static(b"Hello"), 1.0)
        .unwrap();
    stepper.frame_step(1);

    let sent = read(&stepper, true) - client_before;
    let received = read(&stepper, false) - server_before;
    assert_eq!(
        (sent, received),
        (
            ChannelCounts {
                sent_bytes: 5,
                sent_messages: 1,
                received_bytes: 0
            },
            ChannelCounts {
                sent_bytes: 0,
                sent_messages: 0,
                received_bytes: 5
            }
        )
    );
}

/// The senders' fragment size follows the largest payload of the link's session: the one the
/// link holds when the transport comes, then the one it holds at each link.
#[test]
fn fragment_size_follows_the_sessions_largest_payload() {
    let mut world = World::new();
    world.add_observer(Transport::on_linked);
    let mut link = Link::default();
    link.session.set_max_payload(256);
    let entity = world.spawn((link, Transport::default())).id();
    assert_eq!(
        world.get::<Transport>(entity).unwrap().fragment_size(),
        fragment_size_for_max_payload(256).unwrap()
    );

    world
        .get_mut::<Link>(entity)
        .unwrap()
        .session
        .set_max_payload(200);
    world.entity_mut(entity).insert(Linked);
    assert_eq!(
        world.get::<Transport>(entity).unwrap().fragment_size(),
        fragment_size_for_max_payload(200).unwrap()
    );
}
