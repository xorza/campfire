use super::*;
use bevy_ecs::system::RunSystemOnce;
use bytes::BytesMut;
use campfire_app_threads::OneThreadPlugins;
use campfire_net_channels::ChannelTypeId;
use campfire_net_channels::Channels;
use campfire_net_channels::internals::whole_messages;
use campfire_net_core::Instant;
use campfire_net_payload::{OpenedPayload, PacketNumber};
use core::time::Duration;

#[derive(Resource, Default)]
struct Outcomes {
    received: usize,
    unlinked: Vec<UnlinkReason>,
}

/// A packet that breaks the protocol unlinks its link once, as `ProtocolViolation`, and the
/// systems announce no packet: a short tick with no whole one before it.
#[test]
fn a_protocol_violation_unlinks_the_link_once() {
    let mut app = App::new();
    app.add_plugins(OneThreadPlugins);
    let world = app.world_mut();
    world.insert_resource(Clock::frame_at(Instant::ZERO, Duration::ZERO));
    world.init_resource::<Outcomes>();
    world.add_observer(
        |_: On<'_, '_, PacketReceived>, mut outcomes: ResMut<'_, Outcomes>| {
            outcomes.received += 1;
        },
    );
    world.add_observer(
        |unlink: On<'_, '_, Unlink>, mut outcomes: ResMut<'_, Outcomes>| {
            outcomes.unlinked.push(unlink.reason);
        },
    );
    let mut link = Link::default();
    link.packet_session.push_opened(OpenedPayload {
        number: PacketNumber(0),
        kind: whole_messages(),
        payload: BytesMut::from(&[0_u8, 1][..]),
    });
    world.spawn((link, Linked, Transport::from(Channels::default())));
    world
        .run_system_once(TransportPlugin::buffer_receive)
        .unwrap();
    let outcomes = world.resource::<Outcomes>();
    assert_eq!(outcomes.unlinked, [UnlinkReason::ProtocolViolation]);
    assert_eq!(outcomes.received, 0);
}

/// Stopping a server unlinks its IO.
#[test]
fn stopping_the_server_unlinks_it() {
    use crate::fixtures::transport_stack::TransportStack;
    use campfire_net_connection::Stop;
    use campfire_net_connection::Stopped;
    use campfire_net_link::Unlinked;
    use campfire_net_stepper::StepperSetup;
    use campfire_net_stepper::{StepOrder, Stepper};

    let mut stepper = Stepper::new(TransportStack, StepperSetup::single());
    let server = stepper.server_entity();
    stepper.server_world_mut().trigger(Stop::from(server));
    stepper.step(10, stepper.frame_interval(), StepOrder::ServerFirst);

    let server = stepper.server();
    assert!(server.contains::<Stopped>());
    assert!(server.contains::<Unlinked>());
    assert!(!server.contains::<Linked>());
}

#[derive(Component)]
struct Admitted;

/// A channel a server's link receives on only once admitted: the link's transport gets no
/// receiver of it until `Admitted` comes, then one, which stays as `Admitted` goes and comes back,
/// with its state; a link admitted before its transport gets one with it; a client's link sends
/// on it from the start.
#[test]
fn a_gated_channel_opens_to_a_link_once_admitted() {
    use crate::app_channel_ext::AppChannelExt;
    use crate::fixtures::test_transport_plugin::TestChannel;
    use campfire_net_channels::ChannelSetup;
    use campfire_net_connection::{Client, ClientOf};
    use campfire_net_core::LinkDirection;

    let mut app = App::new();
    app.add_plugins(TransportPlugin);
    app.register_channel::<TestChannel>(ChannelSetup::default())
        .add_direction_after::<Admitted>(LinkDirection::ClientToServer);
    let channel = app
        .world()
        .resource::<ChannelRegistry>()
        .net_id(ChannelTypeId::of::<TestChannel>())
        .unwrap();
    let world = app.world_mut();
    let link = world.spawn((ClientOf, Transport::default())).id();
    let client = world.spawn((Client, Transport::default())).id();
    let receives = |world: &mut World| {
        world
            .get_mut::<Transport>(link)
            .unwrap()
            .channel_receiver_mut(channel)
            .is_some()
    };
    assert!(!receives(world));
    world.entity_mut(link).insert(Admitted);
    assert!(receives(world));
    world.entity_mut(link).remove::<Admitted>();
    world.entity_mut(link).insert(Admitted);
    assert!(receives(world));
    // A link admitted before its transport comes takes the receiver with it.
    let early = world.spawn((ClientOf, Admitted)).id();
    world.entity_mut(early).insert(Transport::default());
    let early_receives = world
        .get_mut::<Transport>(early)
        .unwrap()
        .channel_receiver_mut(channel)
        .is_some();
    assert!(early_receives);
    let mut client_transport = world.get_mut::<Transport>(client).unwrap();
    client_transport.send(channel, bytes::Bytes::from_static(&[1]), 1.0);
    assert_eq!(client_transport.refused(), None);
}

/// An open channel's sender comes with a link's transport on the side that sends on it, and its
/// receiver on the side that receives; a second insert of the link's side keeps them with their
/// state: a reliable message buffered before it is still pending after it.
#[test]
fn an_open_channel_comes_with_the_transport_and_keeps_its_state() {
    use crate::app_channel_ext::AppChannelExt;
    use crate::fixtures::protocol::ChannelOrderedReliable;
    use campfire_net_channels::{ChannelMode, ChannelSetup};
    use campfire_net_connection::{Client, ClientOf};
    use campfire_net_core::LinkDirection;

    let mut app = App::new();
    app.add_plugins(TransportPlugin);
    let setup = ChannelSetup {
        mode: ChannelMode::OrderedReliable,
        ..ChannelSetup::default()
    };
    app.register_channel::<ChannelOrderedReliable>(setup)
        .add_direction(LinkDirection::ClientToServer);
    let channel = app
        .world()
        .resource::<ChannelRegistry>()
        .net_id(ChannelTypeId::of::<ChannelOrderedReliable>())
        .unwrap();
    let world = app.world_mut();
    let client = world.spawn((Client, Transport::default())).id();
    let link = world.spawn((ClientOf, Transport::default())).id();
    let mut server = world.get_mut::<Transport>(link).unwrap();
    assert!(server.channel_receiver_mut(channel).is_some());

    let mut transport = world.get_mut::<Transport>(client).unwrap();
    assert!(transport.channel_receiver_mut(channel).is_none());
    transport
        .send_mut(channel, bytes::Bytes::from_static(b"m"), 1.0)
        .unwrap();
    world.entity_mut(client).insert(Client);
    let pending = world.get::<Transport>(client).unwrap().pending_reliable();
    assert_eq!(pending.len(), 1);
}

/// A channel opens behind one gate: its registration open and then behind a gate panics.
#[test]
#[should_panic(expected = "opens behind two gates")]
fn a_channel_of_two_gates_panics() {
    use crate::app_channel_ext::AppChannelExt;
    use crate::fixtures::test_transport_plugin::TestChannel;
    use campfire_net_channels::ChannelSetup;
    use campfire_net_core::LinkDirection;

    let mut app = App::new();
    app.add_plugins(TransportPlugin);
    app.register_channel::<TestChannel>(ChannelSetup::default())
        .add_direction(LinkDirection::ServerToClient)
        .add_direction_after::<Admitted>(LinkDirection::ClientToServer);
}

/// A message a reliable channel cannot take unlinks its link with why, rather than leave its peer
/// a stream with a hole: past the channel's largest, `MessageTooLarge`; past the link's send
/// budget, `Backlogged`. An unreliable channel drops one and keeps its link.
#[test]
fn a_reliable_channel_that_refuses_a_message_unlinks_its_link() {
    use crate::fixtures::protocol::{Channel1, ChannelOrderedReliable, channel_id};
    use crate::fixtures::transport_stack::TransportStack;
    use campfire_net_stepper::{StepOrder, Stepper, StepperSetup};

    let sends = |reliable: bool, len: usize, count: usize| {
        let mut stepper = Stepper::new(TransportStack, StepperSetup::single());
        let world = stepper.server_world_mut();
        world.init_resource::<Outcomes>();
        world.add_observer(
            |unlink: On<'_, '_, Unlink>, mut outcomes: ResMut<'_, Outcomes>| {
                outcomes.unlinked.push(unlink.reason);
            },
        );
        let channel = if reliable {
            channel_id::<ChannelOrderedReliable>(stepper.server_app())
        } else {
            channel_id::<Channel1>(stepper.server_app())
        };
        let mut client = stepper.client_of_mut(0);
        let mut transport = client.get_mut::<Transport>().unwrap();
        for _ in 0..count {
            transport.send(channel, bytes::Bytes::from(vec![0; len]), 1.0);
        }
        stepper.step(2, stepper.frame_interval(), StepOrder::ServerFirst);
        stepper
            .server_world()
            .resource::<Outcomes>()
            .unlinked
            .clone()
    };
    // The send budget is twice the largest message, so two largest fill it and a third passes.
    let largest = campfire_net_channels::ChannelSetup::default().max_message_len;
    assert_eq!(sends(true, largest + 1, 1), [UnlinkReason::MessageTooLarge]);
    assert_eq!(sends(true, largest, 3), [UnlinkReason::Backlogged]);
    assert_eq!(sends(false, largest + 1, 1), []);
    assert_eq!(sends(true, largest, 2), []);
}
