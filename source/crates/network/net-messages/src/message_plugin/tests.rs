use super::*;
use crate::app_event_ext::AppEventExt;
use crate::app_message_ext::AppMessageExt;
use crate::event_sender::EventSender;
use crate::fixtures::host_received_messages::HostReceivedMessages;
use crate::fixtures::messages_stack::MessagesStack;
use crate::fixtures::protocol::EntityMessage;
use crate::fixtures::protocol::StringEvent;
use crate::fixtures::protocol::StringMessage;
use crate::fixtures::received_messages::ReceivedMessages;
use crate::host_event_sender::HostEventSender;
use crate::host_message_sender::HostMessageSender;
use crate::link_messages::LinkEndpoint;
use crate::message_receiver::HostMessageReceiver;
use crate::message_receiver::MessageReceiver;
use crate::message_sender::MessageSender;
use crate::message_type_id::MessageTypeId;
use crate::multi_message_sender::MultiMessageSender;
use crate::received_message::ReceivedMessage;
use crate::remote_event::RemoteEvent;
use bevy_ecs::entity::UniqueEntityArray;
use bevy_ecs::event::Event;
use bevy_ecs::prelude::ResMut;
use bevy_ecs::prelude::Resource;
use bevy_replicon::shared::replication::Replicated;
use campfire_math::TickRate;
use campfire_net_channels::fragment_len_for_max_payload;
use campfire_net_connection::NetworkingState;
use campfire_net_core::LinkDirections;
use campfire_net_link::Link;
use campfire_net_link::LinkPlugin;
use campfire_net_link::Linked;
use campfire_net_payload::PacketSessionQueues;
use campfire_net_stepper::ClientKind;
use campfire_net_stepper::Stepper;
use campfire_net_stepper::StepperSetup;
use campfire_net_timeline::internals::TestTimelinePlugins;
use campfire_net_transport::TransportPlugin;
use campfire_net_transport::internals::Channel1;
use campfire_net_transport::internals::TestChannel;
use campfire_net_transport::internals::TestTransportPlugin;
use serde::Deserialize;
use serde::Serialize;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct M(usize);

#[derive(Event, Serialize, Deserialize, Clone, Debug, PartialEq)]
struct E(usize);

#[derive(Debug, Resource, Default)]
struct EventCount(usize);

fn count_event(_: On<'_, '_, RemoteEvent<E>>, mut count: ResMut<'_, EventCount>) {
    count.0 += 1;
}

fn message_test_app(register_event: bool) -> App {
    let mut app = App::new();
    app.add_plugins(TestTimelinePlugins {
        tick_rate: TickRate::try_from(100).unwrap(),
    });
    app.add_plugins(TestTransportPlugin);
    app.init_resource::<NetworkingState>();
    app.register_message::<M>()
        .add_direction(LinkDirections::BOTH);
    if register_event {
        app.register_event::<E>()
            .add_direction(LinkDirections::BOTH);
        app.init_resource::<EventCount>();
        app.add_observer(count_event);
    }
    app.add_plugins(MessagePlugin);
    app.finish();
    app
}

/// A link whose packet session takes every payload, which a test loops back to itself.
fn loopback_link() -> Link {
    Link {
        packet_session: PacketSessionQueues::unbounded(),
    }
}

fn loopback_link_payload(app: &mut App, entity: Entity) {
    let mut entity_mut = app.world_mut().entity_mut(entity);
    let mut link = entity_mut.get_mut::<Link>().unwrap();
    assert_eq!(link.packet_session.loop_back(), 1, "one outgoing payload");
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct ToClients(usize);

#[derive(Debug, Resource, Default)]
struct Unlinks(Vec<UnlinkReason>);

/// An app with a connected loopback link whose `MessageSender<ToClients>` holds one message: a
/// client's link, or with no `Client` marker a server's.
fn to_clients_link(client: bool) -> (App, Entity) {
    let mut app = App::new();
    app.add_plugins(TestTimelinePlugins {
        tick_rate: TickRate::try_from(100).unwrap(),
    });
    app.add_plugins(TestTransportPlugin);
    app.register_message::<ToClients>()
        .add_direction(LinkDirection::ServerToClient);
    app.add_plugins(MessagePlugin);
    app.init_resource::<Unlinks>();
    app.add_observer(
        |unlink: On<'_, '_, Unlink>, mut unlinks: ResMut<'_, Unlinks>| {
            unlinks.0.push(unlink.reason);
        },
    );
    app.finish();
    let registry = app.world().resource::<ChannelRegistry>();
    let mut transport = Transport::default();
    transport.add_channel_send_from_registry::<TestChannel>(registry);
    transport.add_channel_receive_from_registry::<TestChannel>(registry);
    let entity = app
        .world_mut()
        .spawn((
            loopback_link(),
            transport,
            MessageSender::<ToClients>::default(),
            RemoteId(PeerId::Local(0)),
            Linked,
            Connected,
        ))
        .id();
    if client {
        app.world_mut().entity_mut(entity).insert(Client);
    }
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<MessageSender<ToClients>>()
        .unwrap()
        .send::<TestChannel>(ToClients(1));
    (app, entity)
}

/// A server receives only what clients may send: `ToClients` arriving at a server is a protocol
/// violation, which unlinks the peer and delivers nothing.
#[test]
fn a_message_against_its_direction_unlinks() {
    // A server's link, which sends `ToClients` toward its client: its loopback brings the
    // message back to the server's side, client to server.
    let (mut app, entity) = to_clients_link(false);
    app.update();
    loopback_link_payload(&mut app, entity);
    app.world_mut().run_schedule(PreUpdate);
    app.world_mut().flush();

    assert_eq!(
        app.world().resource::<Unlinks>().0,
        vec![UnlinkReason::ProtocolViolation]
    );
    assert!(
        app.world()
            .get::<MessageReceiver<ToClients>>(entity)
            .is_none_or(MessageReceiver::is_empty)
    );
}

/// A client's link never sends what only a server sends, so a sender of it put there by hand is
/// a bug, which the peer would answer by unlinking.
#[test]
#[should_panic(expected = "a message is sent ClientToServer, against its registered direction")]
fn a_link_does_not_send_against_its_direction() {
    let (mut app, _) = to_clients_link(true);
    app.update();
}

#[test]
fn a_sent_message_is_received_through_the_link() {
    let mut app = App::new();
    app.add_plugins(TestTimelinePlugins {
        tick_rate: TickRate::try_from(100).unwrap(),
    });
    app.add_plugins(TestTransportPlugin);

    // Register the message before adding the MessagePlugin
    app.register_message::<M>();
    app.add_plugins(MessagePlugin);
    app.finish();

    // Add the Transport component with a receiver/sender for channel C, and a receiver/sender for
    // message M
    let registry = app.world().resource::<ChannelRegistry>();
    let mut transport = Transport::default();
    transport.add_channel_send_from_registry::<TestChannel>(registry);
    transport.add_channel_receive_from_registry::<TestChannel>(registry);
    // TODO: are these tests useful? they need so many components from other plugins..
    let mut entity_mut = app.world_mut().spawn((
        loopback_link(),
        transport,
        MessageSender::<M>::default(),
        RemoteId(PeerId::Local(0)),
        Linked,
        Connected,
    ));

    let entity = entity_mut.id();

    // send message
    let messages = vec![M(2), M(3)];
    let mut sender = entity_mut.get_mut::<MessageSender<M>>().unwrap();
    for message in messages.clone() {
        sender.send::<TestChannel>(message);
    }
    app.update();

    // the payload went to the packet session, which loops it back to the receive side of the link
    loopback_link_payload(&mut app, entity);

    app.world_mut().run_schedule(PreUpdate);

    // check that the message has been received
    let received_messages = app
        .world_mut()
        .entity_mut(entity)
        .get_mut::<MessageReceiver<M>>()
        .unwrap()
        .receive()
        .collect::<Vec<_>>();
    assert_eq!(messages, received_messages);

    app.update();

    // check that the message has been dropped
    assert_eq!(
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<MessageReceiver<M>>()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn an_unread_message_is_dropped_at_the_frame_end() {
    let mut app = App::new();
    app.add_plugins(TestTimelinePlugins {
        tick_rate: TickRate::try_from(100).unwrap(),
    });
    app.register_message::<M>();
    app.add_plugins((LinkPlugin, TransportPlugin, MessagePlugin));
    app.finish();

    let entity_mut = app.world_mut().spawn((
        MessageReceiver::<M>::default(),
        RemoteId(PeerId::Local(0)),
        Connected,
    ));

    let entity = entity_mut.id();

    app.world_mut()
        .entity_mut(entity)
        .get_mut::<MessageReceiver<M>>()
        .unwrap()
        .push_received(ReceivedMessage {
            data: M(2),
            remote_tick: NetTick::default(),
            channel_type_id: ChannelTypeId::of::<TestChannel>(),
            message_id: None,
        });
    app.update();

    // check that the message has been dropped
    assert_eq!(
        app.world_mut()
            .entity_mut(entity)
            .get_mut::<MessageReceiver<M>>()
            .unwrap()
            .len(),
        0
    );
}

#[test]
fn message_is_ready_in_the_receive_phase_it_arrives_in() {
    let mut app = message_test_app(false);
    app.world_mut().resource_mut::<LocalTimeline>().shift(10);
    let registry = app.world().resource::<ChannelRegistry>();
    let mut transport = Transport::default();
    transport.add_channel_send_from_registry::<TestChannel>(registry);
    transport.add_channel_receive_from_registry::<TestChannel>(registry);
    let entity = app
        .world_mut()
        .spawn((
            loopback_link(),
            transport,
            MessageSender::<M>::default(),
            RemoteId(PeerId::Local(0)),
            Linked,
            Connected,
        ))
        .id();
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<MessageSender<M>>()
        .unwrap()
        .send::<TestChannel>(M(8));
    app.update();
    loopback_link_payload(&mut app, entity);
    app.world_mut().run_schedule(PreUpdate);

    let mut entity_mut = app.world_mut().entity_mut(entity);
    let mut receiver = entity_mut.get_mut::<MessageReceiver<M>>().unwrap();
    let received = receiver.receive_with_tick().collect::<Vec<_>>();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].data, M(8));
    // The sender's local tick, 10, travels with the message.
    assert_eq!(received[0].remote_tick, NetTick(10));
}

/// On a host client, what the server's link sends lands in the host client's own receiver, and
/// what the host client sends lands in the link's, each in the next frame's receive, as a remote
/// link's would, and with the sender's tick.
#[test]
fn host_client_messages_land_on_the_other_side_in_the_next_receive() {
    let mut app = message_test_app(false);
    app.world_mut().resource_mut::<LocalTimeline>().shift(10);
    let entity = app
        .world_mut()
        .spawn((
            MessageSender::<M>::default(),
            HostClient { buffer: Vec::new() },
            RemoteId(PeerId::Local(0)),
            Connected,
        ))
        .id();
    // Apply the sender hooks so LinkMessages knows about them.
    app.world_mut().flush();
    let mut entity_mut = app.world_mut().entity_mut(entity);
    entity_mut
        .get_mut::<MessageSender<M>>()
        .unwrap()
        .send::<TestChannel>(M(10));
    entity_mut
        .get_mut::<HostMessageSender<M>>()
        .unwrap()
        .send::<TestChannel>(M(11));
    app.update();
    assert!(
        !app.world()
            .entity(entity)
            .contains::<HostMessageReceiver<M>>()
    );
    app.world_mut().run_schedule(PreUpdate);

    let mut entity_mut = app.world_mut().entity_mut(entity);
    let to_host = entity_mut
        .get_mut::<HostMessageReceiver<M>>()
        .unwrap()
        .receive_with_tick()
        .map(|received| (received.data, received.remote_tick))
        .collect::<Vec<_>>();
    assert_eq!(to_host, [(M(10), NetTick(10))]);
    let to_server = entity_mut
        .get_mut::<MessageReceiver<M>>()
        .unwrap()
        .receive_with_tick()
        .map(|received| (received.data, received.remote_tick))
        .collect::<Vec<_>>();
    assert_eq!(to_server, [(M(11), NetTick(10))]);
}

#[derive(Debug, Resource, Default)]
struct EventSenders(Vec<PeerId>);

/// On a host client, an event the server's link triggers comes from the server, and one the host
/// client triggers comes from the host client's id.
#[test]
fn host_client_events_come_from_their_side() {
    let mut app = message_test_app(true);
    app.init_resource::<EventSenders>();
    app.add_observer(
        |trigger: On<'_, '_, RemoteEvent<E>>, mut senders: ResMut<'_, EventSenders>| {
            senders.0.push(trigger.from);
        },
    );
    let entity = app
        .world_mut()
        .spawn((
            EventSender::<E>::default(),
            HostClient { buffer: Vec::new() },
            RemoteId(PeerId::Local(0)),
            Connected,
        ))
        .id();
    app.world_mut().flush();
    let mut entity_mut = app.world_mut().entity_mut(entity);
    entity_mut
        .get_mut::<EventSender<E>>()
        .unwrap()
        .trigger::<TestChannel>(E(1));
    entity_mut
        .get_mut::<HostEventSender<E>>()
        .unwrap()
        .trigger::<TestChannel>(E(2));

    app.update();
    app.world_mut().run_schedule(PreUpdate);
    assert_eq!(
        app.world().resource::<EventSenders>().0,
        [PeerId::Server, PeerId::Local(0)]
    );
}

#[test]
fn a_direction_adds_the_senders_and_not_the_receivers() {
    let mut app = message_test_app(true);
    let client = app.world_mut().spawn(Client).id();
    app.world_mut().flush();

    let client = app.world().entity(client);
    assert!(client.contains::<MessageSender<M>>());
    assert!(client.contains::<EventSender<E>>());
    assert!(!client.contains::<MessageReceiver<M>>());
}

/// An event sent on a channel the transport lacks is a registration bug, which crashes instead of
/// dropping the event.
#[test]
#[should_panic(expected = "a message is sent only on a channel its transport sends on")]
fn an_event_on_a_channel_the_transport_lacks_panics() {
    let mut app = message_test_app(true);
    let entity = app
        .world_mut()
        .spawn((
            loopback_link(),
            Transport::default(),
            EventSender::<E>::default(),
            RemoteId(PeerId::Local(0)),
            Linked,
            Connected,
        ))
        .id();
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<EventSender<E>>()
        .unwrap()
        .trigger::<TestChannel>(E(1));

    app.update();
}

/// A sender or receiver an app adds is listed once, by its net id and user; one an app removes
/// leaves its link's lists, so the frame's send and clear passes do not look for it.
#[test]
fn a_removed_endpoint_leaves_the_link_messages() {
    let mut app = message_test_app(true);
    let entity = app
        .world_mut()
        .spawn((
            MessageReceiver::<M>::default(),
            MessageSender::<M>::default(),
            EventSender::<E>::default(),
            RemoteId(PeerId::Local(1)),
            Connected,
        ))
        .id();
    let link_messages = app.world().get::<LinkMessages>(entity).unwrap();
    let registry = app.world().resource::<MessageRegistry>();
    let link = |type_id| LinkEndpoint {
        net_id: registry.net_id(type_id).unwrap(),
        user: EndpointUser::Link,
    };
    assert_eq!(link_messages.receivers, [link(MessageTypeId::of::<M>())]);
    assert_eq!(
        link_messages.senders,
        [
            link(MessageTypeId::of::<M>()),
            link(MessageTypeId::of::<E>())
        ]
    );

    app.world_mut()
        .entity_mut(entity)
        .remove::<(MessageReceiver<M>, MessageSender<M>, EventSender<E>)>();
    app.update();

    let link_messages = app.world().get::<LinkMessages>(entity).unwrap();
    assert!(link_messages.receivers.is_empty());
    assert!(link_messages.senders.is_empty());
}

#[test]
fn a_disconnect_removes_the_message_receivers() {
    let mut app = message_test_app(true);
    let entity = app
        .world_mut()
        .spawn((
            MessageReceiver::<M>::default(),
            RemoteId(PeerId::Local(1)),
            Connected,
        ))
        .id();
    app.world_mut()
        .entity_mut(entity)
        .get_mut::<MessageReceiver<M>>()
        .unwrap()
        .push_received(ReceivedMessage {
            data: M(0),
            remote_tick: NetTick(1),
            channel_type_id: ChannelTypeId::of::<TestChannel>(),
            message_id: None,
        });
    app.world_mut()
        .entity_mut(entity)
        .insert(Disconnected::default());

    app.world_mut().run_schedule(PreUpdate);
    let entity_ref = app.world().entity(entity);
    assert!(!entity_ref.contains::<MessageReceiver<M>>());
    assert!(
        entity_ref
            .get::<LinkMessages>()
            .unwrap()
            .receivers
            .is_empty()
    );
}

/// Each end of a messages-stack stepper of one client of `kind`: the entity that sends, and the
/// entity the other side receives on.
struct Ends {
    client: Entity,
    server: Entity,
}

fn ends(stepper: &Stepper<MessagesStack>, kind: ClientKind) -> Ends {
    match kind {
        ClientKind::Plain | ClientKind::Udp => Ends {
            client: stepper.client_entity(0),
            server: stepper.client_of_entity(0),
        },
        // A host client is its own server link.
        ClientKind::Host => Ends {
            client: stepper.host_client_entity(),
            server: stepper.host_client_entity(),
        },
    }
}

fn client_app(stepper: &mut Stepper<MessagesStack>, kind: ClientKind) -> &mut App {
    match kind {
        ClientKind::Plain | ClientKind::Udp => stepper.client_app_mut(0),
        ClientKind::Host => stepper.server_app_mut(),
    }
}

/// A message crosses from the client to the server, and from the server to the client, for a
/// remote client and for a host client, whose server link is the client entity itself: the host
/// client sends through its own side's sender, and reads its own side's receiver.
#[test]
fn messages_cross_both_ways_for_each_client_kind() {
    for kind in [ClientKind::Plain, ClientKind::Udp, ClientKind::Host] {
        let mut stepper = Stepper::new(MessagesStack, StepperSetup::with_clients(vec![kind]));
        let Ends { client, server } = ends(&stepper, kind);
        ReceivedMessages::<StringMessage>::collect_in(stepper.server_app_mut());
        match kind {
            ClientKind::Plain | ClientKind::Udp => {
                ReceivedMessages::<StringMessage>::collect_in(client_app(&mut stepper, kind));
            }
            ClientKind::Host => {
                HostReceivedMessages::<StringMessage>::collect_in(client_app(&mut stepper, kind));
            }
        }

        let hello = StringMessage("Hello".to_string());
        let mut client_mut = client_app(&mut stepper, kind)
            .world_mut()
            .entity_mut(client);
        match kind {
            ClientKind::Plain | ClientKind::Udp => client_mut
                .get_mut::<MessageSender<StringMessage>>()
                .unwrap()
                .send::<Channel1>(hello.clone()),
            ClientKind::Host => client_mut
                .get_mut::<HostMessageSender<StringMessage>>()
                .unwrap()
                .send::<Channel1>(hello.clone()),
        }
        stepper.frame_step(2);
        let received = core::mem::take(
            &mut stepper
                .server_app_mut()
                .world_mut()
                .resource_mut::<ReceivedMessages<StringMessage>>()
                .0,
        );
        assert_eq!(received, [(server, hello)], "{kind:?}");

        // A remote client's message of one byte past two fragments of its packet session's largest
        // payload crosses whole in three fragments.
        if kind != ClientKind::Host {
            let max_payload = stepper
                .client(0)
                .get::<Link>()
                .unwrap()
                .packet_session
                .max_payload();
            let fragment = fragment_len_for_max_payload(max_payload).unwrap();
            let long = StringMessage("ab".repeat(fragment) + "c");
            stepper
                .client_mut(0)
                .get_mut::<MessageSender<StringMessage>>()
                .unwrap()
                .send::<Channel1>(long.clone());
            stepper.frame_step(2);
            let received = core::mem::take(
                &mut stepper
                    .server_app_mut()
                    .world_mut()
                    .resource_mut::<ReceivedMessages<StringMessage>>()
                    .0,
            );
            assert_eq!(received, [(server, long)], "{kind:?}");
        }

        let world = StringMessage("World".to_string());
        let message = world.clone();
        let system = stepper.server_app_mut().register_system(
            move |mut sender: MultiMessageSender<'_, '_>| {
                sender
                    .send::<_, Channel1>(&message, UniqueEntityArray::from([server]))
                    .unwrap();
            },
        );
        stepper
            .server_app_mut()
            .world_mut()
            .run_system(system)
            .unwrap();
        stepper.frame_step(2);
        let client_world = client_app(&mut stepper, kind).world();
        let received = match kind {
            ClientKind::Plain | ClientKind::Udp => {
                &client_world.resource::<ReceivedMessages<StringMessage>>().0
            }
            ClientKind::Host => {
                &client_world
                    .resource::<HostReceivedMessages<StringMessage>>()
                    .0
            }
        };
        assert_eq!(received, &[(client, world)], "{kind:?}");
        if kind == ClientKind::Host {
            assert!(
                client_world
                    .resource::<ReceivedMessages<StringMessage>>()
                    .0
                    .is_empty(),
                "the server's message to its host client stays out of the server side"
            );
        }
    }
}

/// Only a client maps the entities a message names: the client writes its entities as the
/// server's, and the server reads and writes its own, which the client reads back as its own. The
/// server accepts from the client only an entity it replicates or the link's own, so a stale or
/// forged one, here the server-only entity, is the placeholder; an entity that the client cannot
/// map goes as the placeholder each way. A host client shares its server's world, so its entities
/// cross unchanged.
#[test]
fn only_a_client_maps_the_entities_a_message_names() {
    for kind in [ClientKind::Plain, ClientKind::Udp, ClientKind::Host] {
        let mut stepper = Stepper::new(MessagesStack, StepperSetup::with_clients(vec![kind]));
        let Ends { client, server } = ends(&stepper, kind);
        let replicated = stepper.server_app_mut().world_mut().spawn(Replicated).id();
        let server_only = stepper.server_app_mut().world_mut().spawn_empty().id();
        let unmapped = stepper.server_app_mut().world_mut().spawn_empty().id();
        let mut spawn = || {
            client_app(&mut stepper, kind)
                .world_mut()
                .spawn_empty()
                .id()
        };
        let (copy, stale, client_only) = (spawn(), spawn(), spawn());
        ReceivedMessages::<EntityMessage>::collect_in(stepper.server_app_mut());
        let (client_sends, server_reads, server_sends, client_reads) = match kind {
            ClientKind::Plain | ClientKind::Udp => {
                ReceivedMessages::<EntityMessage>::collect_in(client_app(&mut stepper, kind));
                let mut map = stepper.client_mut(0);
                let map = &mut map.get_mut::<LinkMessages>().unwrap().entity_mapper;
                map.insert(server, client);
                map.insert(replicated, copy);
                map.insert(server_only, stale);
                let mut map = stepper.client_of_mut(0);
                let map = &mut map.get_mut::<LinkMessages>().unwrap().entity_mapper;
                map.insert(client, server);
                (
                    vec![client, copy, stale, client_only],
                    vec![server, replicated, Entity::PLACEHOLDER, Entity::PLACEHOLDER],
                    vec![server, replicated, server_only, unmapped],
                    vec![client, copy, stale, Entity::PLACEHOLDER],
                )
            }
            ClientKind::Host => {
                HostReceivedMessages::<EntityMessage>::collect_in(client_app(&mut stepper, kind));
                (
                    vec![client, client_only],
                    vec![client, client_only],
                    vec![server, server_only],
                    vec![server, server_only],
                )
            }
        };

        let mut client_mut = client_app(&mut stepper, kind)
            .world_mut()
            .entity_mut(client);
        for &entity in &client_sends {
            match kind {
                ClientKind::Plain | ClientKind::Udp => client_mut
                    .get_mut::<MessageSender<EntityMessage>>()
                    .unwrap()
                    .send::<Channel1>(EntityMessage(entity)),
                ClientKind::Host => client_mut
                    .get_mut::<HostMessageSender<EntityMessage>>()
                    .unwrap()
                    .send::<Channel1>(EntityMessage(entity)),
            }
        }
        stepper.frame_step(2);
        // `Channel1` keeps no order.
        let read = |received: &[(Entity, EntityMessage)]| {
            let mut entities: Vec<_> = received.iter().map(|(_, message)| message.0).collect();
            entities.sort();
            entities
        };
        let sorted = |mut entities: Vec<Entity>| {
            entities.sort();
            entities
        };
        let server_world = stepper.server_app().world();
        assert_eq!(
            read(&server_world.resource::<ReceivedMessages<EntityMessage>>().0),
            sorted(server_reads),
            "{kind:?}"
        );

        let system = stepper.server_app_mut().register_system(
            move |mut sender: MultiMessageSender<'_, '_>| {
                for &entity in &server_sends {
                    sender
                        .send::<_, Channel1>(
                            &EntityMessage(entity),
                            UniqueEntityArray::from([server]),
                        )
                        .unwrap();
                }
            },
        );
        stepper
            .server_app_mut()
            .world_mut()
            .run_system(system)
            .unwrap();
        stepper.frame_step(2);
        let client_world = client_app(&mut stepper, kind).world();
        let received = match kind {
            ClientKind::Plain | ClientKind::Udp => {
                &client_world.resource::<ReceivedMessages<EntityMessage>>().0
            }
            ClientKind::Host => {
                &client_world
                    .resource::<HostReceivedMessages<EntityMessage>>()
                    .0
            }
        };
        assert_eq!(read(received), sorted(client_reads), "{kind:?}");
    }
}

#[derive(Debug, Resource, Default)]
struct ReceivedEvents(Vec<(Entity, StringEvent)>);

fn collect_events(
    trigger: On<'_, '_, RemoteEvent<StringEvent>>,
    networking: Res<'_, NetworkingState>,
    mut received: ResMut<'_, ReceivedEvents>,
) {
    let from = *networking.peer_map.get(&trigger.from).unwrap();
    received.0.push((from, trigger.event.clone()));
}

/// An event a client triggers reaches the server as from that client's link, for each kind; a
/// host client triggers through its own side's sender.
#[test]
fn events_reach_the_server_for_each_client_kind() {
    for kind in [ClientKind::Plain, ClientKind::Udp, ClientKind::Host] {
        let mut stepper = Stepper::new(MessagesStack, StepperSetup::with_clients(vec![kind]));
        let Ends { client, server } = ends(&stepper, kind);
        stepper
            .server_app_mut()
            .init_resource::<ReceivedEvents>()
            .add_observer(collect_events);

        let hello = StringEvent("Hello".to_string());
        let mut client_mut = client_app(&mut stepper, kind)
            .world_mut()
            .entity_mut(client);
        match kind {
            ClientKind::Plain | ClientKind::Udp => client_mut
                .get_mut::<EventSender<StringEvent>>()
                .unwrap()
                .trigger::<Channel1>(hello.clone()),
            ClientKind::Host => client_mut
                .get_mut::<HostEventSender<StringEvent>>()
                .unwrap()
                .trigger::<Channel1>(hello.clone()),
        }
        stepper.frame_step(2);
        let received = &stepper.server_app().world().resource::<ReceivedEvents>().0;
        assert_eq!(received, &[(server, hello)], "{kind:?}");
    }
}

/// A link that is not connected when the frame sends drops its queues, messages and events
/// alike: what client 0 queues after it disconnected is gone a frame later.
#[test]
fn queues_of_a_link_that_is_not_connected_are_dropped() {
    let mut stepper = Stepper::new(MessagesStack, StepperSetup::with_plain_clients(1));
    stepper.disconnect(0);
    let mut client = stepper.client_mut(0);
    client
        .get_mut::<MessageSender<StringMessage>>()
        .unwrap()
        .send::<Channel1>(StringMessage("late".to_string()));
    client
        .get_mut::<EventSender<StringEvent>>()
        .unwrap()
        .trigger::<Channel1>(StringEvent("late".to_string()));

    stepper.frame_step(1);

    let client = stepper.client(0);
    assert_eq!(
        client.get::<MessageSender<StringMessage>>().unwrap().len(),
        0
    );
    assert_eq!(client.get::<EventSender<StringEvent>>().unwrap().len(), 0);
}
