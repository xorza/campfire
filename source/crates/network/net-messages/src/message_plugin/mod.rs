use crate::endpoint_side::EndpointSide;
use crate::message_error::MessageError;
use crate::message_manager::MessageManager;
use crate::message_receive_context::MessageReceiveContext;
use crate::message_receive_context::ReceivedMessageHeader;
use crate::message_registry::MessageModeFns;
use crate::message_registry::MessageRegistry;
use crate::message_systems::MessageSystems;
use crate::receive_map_view::ReceiveMapView;
use crate::replicon_map_param::RepliconMapParam;
use crate::send_map_view::SendMapView;
use crate::send_queue::SendLink;
use crate::send_queue::SendTarget;
use bevy_app::App;
use bevy_app::Last;
use bevy_app::Plugin;
use bevy_app::PostUpdate;
use bevy_app::PreUpdate;
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::Add;
use bevy_ecs::prelude::Commands;
use bevy_ecs::prelude::On;
use bevy_ecs::prelude::Res;
use bevy_ecs::query::Has;
use bevy_ecs::query::With;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::ParallelCommands;
use bevy_ecs::system::ParamBuilder;
use bevy_ecs::system::Query;
use bevy_ecs::system::QueryParamBuilder;
use bevy_ecs::system::SystemParamBuilder;
use bevy_ecs::world::FilteredEntityMut;
use bevy_ecs::world::World;
use bytes::Bytes;
use campfire_net_bytes::Reader;
use campfire_net_bytes::ToBytes;
use campfire_net_channels::ChannelKind;
use campfire_net_channels::ChannelMessage;
use campfire_net_channels::MessageId;
use campfire_net_connection::Client;
use campfire_net_connection::ClientOf;
use campfire_net_connection::Connected;
use campfire_net_connection::Disconnected;
use campfire_net_connection::HostClient;
use campfire_net_connection::RemoteId;
use campfire_net_core::Direction;
use campfire_net_core::NetId;
use campfire_net_core::NetTick;
use campfire_net_core::PeerId;
use campfire_net_link::Unlink;
use campfire_net_payload::UnlinkReason;
use campfire_net_timeline::LocalTimeline;
use campfire_net_transport::AdaptiveQueryIterMut;
use campfire_net_transport::ChannelRegistry;
use campfire_net_transport::Transport;
use campfire_net_transport::TransportPlugin;
use campfire_net_transport::TransportSystems;
use std::sync::Arc;
use tracing::debug;
use tracing::error;
use tracing::trace;

/// Receives and sends each link's registered messages and events over its transport.
#[derive(Debug)]
pub struct MessagePlugin;

impl Plugin for MessagePlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<TransportPlugin>() {
            app.add_plugins(TransportPlugin);
        }

        app.init_resource::<MessageRegistry>();
        app.add_observer(Self::handle_disconnection);

        app.register_required_components::<Client, MessageManager>();

        app.register_required_components::<ClientOf, MessageManager>();
    }

    // NOTE: this should only be called once all messages are registered, because we use the list of
    // registered messages to provide the dynamic access
    fn finish(&self, app: &mut App) {
        let mut registry = app
            .world_mut()
            .remove_resource::<MessageRegistry>()
            .unwrap();
        registry.seal();

        let receive = (
            QueryParamBuilder::new(|builder| {
                builder.optional(|b| {
                    for &component_id in registry.receive_component_ids().iter() {
                        b.mut_id(component_id);
                    }
                });
            }),
            ParamBuilder,
            ParamBuilder,
            ParamBuilder,
            // RepliconMapParam: replicon's shared map, and the server's entity check
            ParamBuilder,
        )
            .build_state(app.world_mut())
            .build_system(Self::receive)
            .with_name("MessagePlugin::receive");

        let clear = (
            QueryParamBuilder::new(|builder| {
                builder.optional(|b| {
                    for &component_id in registry.receive_component_ids().iter() {
                        b.mut_id(component_id);
                    }
                });
            }),
            ParamBuilder,
        )
            .build_state(app.world_mut())
            .build_system(Self::clear)
            .with_name("MessagePlugin::clear");

        let send = (
            ParamBuilder,
            QueryParamBuilder::new(|builder| {
                builder.optional(|b| {
                    registry.messages().for_each(|message| {
                        for component_id in message.send().component_ids() {
                            b.mut_id(component_id);
                        }
                    });
                });
            }),
            ParamBuilder,
            ParamBuilder,
            // RepliconMapParam: replicon's shared map, and the server's entity check
            ParamBuilder,
        )
            .build_state(app.world_mut())
            .build_system(Self::send)
            .with_name("MessagePlugin::send");

        app.configure_sets(
            PreUpdate,
            MessageSystems::Receive.after(TransportSystems::Receive),
        );
        app.configure_sets(
            PostUpdate,
            MessageSystems::Send.before(TransportSystems::Send),
        );
        app.add_systems(PreUpdate, receive.in_set(MessageSystems::Receive));
        app.add_systems(PostUpdate, send.in_set(MessageSystems::Send));
        app.add_systems(Last, clear);

        app.world_mut().insert_resource(registry);
    }
}

/// The link a pass of [`MessagePlugin::receive`] reads, and what every message on it shares.
struct ReceiveLink<'a, 'w, 's> {
    entity: Entity,
    /// The side of the entity whose receivers the messages land on.
    side: EndpointSide,
    peer: PeerId,
    /// The direction every message on this link travels: toward a client or toward a server.
    arriving: Direction,
    registry: &'a MessageRegistry,
    entity_map: &'a ReceiveMapView<'a>,
    commands: &'a ParallelCommands<'w, 's>,
}

impl core::fmt::Debug for ReceiveLink<'_, '_, '_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReceiveLink")
            .field("entity", &self.entity)
            .field("side", &self.side)
            .field("peer", &self.peer)
            .field("arriving", &self.arriving)
            .finish_non_exhaustive()
    }
}

/// One message's bytes, as a channel delivered them.
#[derive(Debug)]
struct ArrivedMessage {
    bytes: Bytes,
    channel_kind: ChannelKind,
    channel_name: &'static str,
    tick: NetTick,
    message_id: Option<MessageId>,
}

impl MessagePlugin {
    /// Resets receive state when a connection ends.
    ///
    /// Typed message receivers are recreated lazily, so removing them discards the payloads of
    /// the previous connection.
    fn handle_disconnection(
        trigger: On<'_, '_, Add<Disconnected>>,
        registry: Option<Res<'_, MessageRegistry>>,
        mut commands: Commands<'_, '_>,
    ) {
        let Some(registry) = registry else {
            return;
        };
        let receive_component_ids = Arc::clone(registry.receive_component_ids());
        let entity = trigger.entity;
        commands.queue(move |world: &mut World| {
            let Ok(mut entity_mut) = world.get_entity_mut(entity) else {
                return;
            };
            if let Some(mut manager) = entity_mut.get_mut::<MessageManager>() {
                manager.entity_mapper.clear();
            }
            entity_mut.remove_by_ids(&receive_component_ids);
        });
    }
    /// Delivers one message; an `Err` is the peer's protocol violation: bytes that do not
    /// decode, an unknown message, or a message against its registered direction.
    fn receive_message_bytes(
        link: &ReceiveLink<'_, '_, '_>,
        receivers: &mut FilteredEntityMut<'_, '_>,
        message: ArrivedMessage,
    ) -> Result<(), MessageError> {
        let ReceiveLink {
            entity,
            side,
            peer,
            arriving,
            registry,
            entity_map,
            commands,
        } = *link;
        let ArrivedMessage {
            bytes,
            channel_kind,
            channel_name,
            tick,
            message_id,
        } = message;
        trace!(
            "Received message (id:{message_id:?}) from peer {:?} on channel {channel_kind:?}. {entity:?}",
            peer
        );
        let mut reader = Reader::from(bytes);
        let net_id = NetId::from_bytes(&mut reader)?;
        let registered = registry
            .get(net_id)
            .ok_or(MessageError::UnrecognizedNetId(net_id))?;
        trace!(
            target: "campfire_net_debug::message",
            kind = "message_receive_bytes",
            schedule = "PreUpdate",
            sample_point = "PreUpdate",
            entity = ?entity,
            message_name = registry.name(net_id),
            message_net_id = net_id.get(),
            channel = channel_name,
            remote_tick = tick.0,
            message_id = ?message_id,
            remote_peer = ?peer,
            "received message bytes"
        );
        if !registered.travels(arriving) {
            return Err(MessageError::WrongDirection(net_id));
        }
        let context = MessageReceiveContext {
            commands,
            link: entity,
            side,
            peer,
            reader: &mut reader,
            header: ReceivedMessageHeader {
                remote_tick: tick,
                channel_kind,
                message_id,
            },
            channel_name,
            serialize_fns: &registered.serialize_fns,
            entity_map,
        };
        match &registered.mode {
            MessageModeFns::Message { receive, .. } => {
                (receive.receive_message_fn)(receivers, context)
            }
            MessageModeFns::Event { receive, .. } => (receive.receive_event_fn)(context),
        }
    }

    /// Deserializes the messages each channel of the [`Transport`] received, or a host client's
    /// buffer holds. A registered event triggers a
    /// [`RemoteEvent`](crate::remote_event::RemoteEvent) through `Commands`; any other message is
    /// buffered in its `MessageReceiver<M>`.
    pub(crate) fn receive(
        mut links: Query<
            '_,
            '_,
            (
                Entity,
                &MessageManager,
                &mut Transport,
                &RemoteId,
                Option<&mut HostClient>,
                Has<Client>,
                // The link's receivers of every registered message.
                FilteredEntityMut<'_, '_>,
            ),
            With<Connected>,
        >,
        registry: Res<'_, MessageRegistry>,
        channel_registry: Res<'_, ChannelRegistry>,
        commands: ParallelCommands<'_, '_>,
        replicon: RepliconMapParam<'_, '_>,
    ) {
        let replicon = &replicon;
        AdaptiveQueryIterMut::new(&mut links).for_each(
            |(
                entity,
                message_manager,
                mut transport,
                peer,
                host_client,
                is_client,
                mut receivers,
            )| {
                if let Some(mut host_client) = host_client {
                    // A host client shares its server's world, so its entities need no map.
                    for payload in host_client.buffer.drain(..) {
                        let side = EndpointSide::receiving(payload.direction);
                        // What the server's link sends comes from the server, and what the host
                        // client sends from the host client.
                        let from = match side {
                            EndpointSide::HostClient => PeerId::Server,
                            EndpointSide::Link => peer.0,
                        };
                        let link = ReceiveLink {
                            entity,
                            side,
                            peer: from,
                            arriving: payload.direction,
                            registry: &registry,
                            entity_map: &ReceiveMapView::AsIs,
                            commands: &commands,
                        };
                        let message = ArrivedMessage {
                            bytes: payload.bytes,
                            channel_kind: channel_registry
                                .kind(payload.channel)
                                .expect("a host client's message names a registered channel"),
                            channel_name: channel_registry.name(payload.channel),
                            tick: payload.tick,
                            message_id: None,
                        };
                        // The host's own bytes failing to decode is a bug on this side, which
                        // drops that message alone.
                        if let Err(error) =
                            Self::receive_message_bytes(&link, &mut receivers, message)
                        {
                            error!(?error, "dropped a host-client message that did not decode");
                        }
                    }
                    return;
                }
                let map = &message_manager.entity_mapper;
                let view = if is_client {
                    replicon.receive_view(map.remote_to_local())
                } else {
                    replicon.checked_view(map.local_to_remote())
                };
                let link = ReceiveLink {
                    entity,
                    side: EndpointSide::Link,
                    peer: peer.0,
                    arriving: if is_client {
                        Direction::ServerToClient
                    } else {
                        Direction::ClientToServer
                    },
                    registry: &registry,
                    entity_map: &view,
                    commands: &commands,
                };
                for channel_receiver in transport.channel_receivers_mut() {
                    let channel_kind = channel_receiver.channel_kind();
                    let channel_name = channel_receiver.name();
                    while let Some(ChannelMessage {
                        tick,
                        bytes,
                        message_id,
                    }) = channel_receiver.read_message()
                    {
                        let message = ArrivedMessage {
                            bytes,
                            channel_kind,
                            channel_name,
                            tick,
                            message_id,
                        };
                        if let Err(error) =
                            Self::receive_message_bytes(&link, &mut receivers, message)
                        {
                            debug!(?entity, ?error, "unlinking a peer that broke the protocol");
                            commands.command_scope(|mut commands| {
                                commands.trigger(Unlink {
                                    entity,
                                    reason: UnlinkReason::ProtocolViolation,
                                });
                            });
                            return;
                        }
                    }
                }
            },
        );
    }

    /// Clear all the message receivers to prevent messages from accumulating
    pub fn clear(
        mut links: Query<
            '_,
            '_,
            (
                &MessageManager,
                // The link's receivers of every registered message.
                FilteredEntityMut<'_, '_>,
            ),
            With<Connected>,
        >,
        registry: Res<'_, MessageRegistry>,
    ) {
        for (manager, mut receivers) in &mut links {
            for receiver in &manager.receivers {
                let receive = registry
                    .message(receiver.net_id)
                    .receive_message()
                    .expect("only a message has a receiver");
                (receive.message_clear_fn)(&mut receivers, receiver.side);
            }
        }
    }
    /// Take the queued messages and events of each link's senders, serialize them, and buffer
    /// them in its [`Transport`], or in a host client's buffer, which the other side of its
    /// entity receives from.
    ///
    /// A link that is not connected when the frame sends, before it connects or after it
    /// disconnects, sends nothing: its queues are dropped, so that they do not outlive a session
    /// or grow while there is none.
    pub(crate) fn send(
        timeline: Res<'_, LocalTimeline>,
        mut links: Query<
            '_,
            '_,
            (
                Entity,
                &mut Transport,
                &mut MessageManager,
                Has<Client>,
                Has<Connected>,
                Option<&mut HostClient>,
                // The link's senders of every registered message.
                FilteredEntityMut<'_, '_>,
            ),
        >,
        registry: Res<'_, MessageRegistry>,
        channels: Res<'_, ChannelRegistry>,
        replicon: RepliconMapParam<'_, '_>,
    ) {
        let tick = timeline.tick();
        let replicon = &replicon;
        let registry = &registry;
        let channels = &channels;
        AdaptiveQueryIterMut::new(&mut links).for_each(
            |(
                entity,
                mut transport,
                mut message_manager,
                is_client,
                connected,
                mut host_client,
                mut endpoints,
            )| {
                if !connected {
                    Self::discard_unsent(&mut endpoints, entity, &message_manager, registry);
                    return;
                }
                let MessageManager {
                    senders,
                    writer,
                    entity_mapper,
                    ..
                } = &mut *message_manager;
                // A host client shares its server's world, so its entities need no map.
                let view = if is_client && host_client.is_none() {
                    replicon.send_view(entity_mapper.local_to_remote())
                } else {
                    SendMapView::AsIs
                };

                // A sender that fails is skipped alone; the senders after it still send.
                for sender in senders.iter() {
                    let registered = registry.message(sender.net_id);
                    let fns = registered.send().side(sender.side);
                    // Only a host client sends from its own side.
                    if host_client.is_none() && sender.side == EndpointSide::HostClient {
                        let _ = (fns.discard_fn)(&mut endpoints);
                        continue;
                    }
                    let direction = sender.side.sends(is_client && host_client.is_none());
                    // A host client holds the senders of both its roles, so the side that does
                    // not send a message keeps an empty sender of it.
                    if !registered.travels(direction) {
                        let dropped = (fns.discard_fn)(&mut endpoints);
                        debug_assert_eq!(
                            dropped, 0,
                            "a message is sent {direction:?}, against its registered direction: \
                             on a host client, the client's own systems send through its host \
                             sender"
                        );
                        continue;
                    }
                    let target = match host_client.as_deref_mut() {
                        Some(host_client) => SendTarget::Host {
                            buffer: &mut host_client.buffer,
                            tick,
                            direction,
                        },
                        None => SendTarget::Transport(&mut transport),
                    };
                    let mut link = SendLink {
                        writer,
                        entity_map: &view,
                        channels,
                        target,
                    };
                    (fns.send_fn)(
                        &mut endpoints,
                        sender.net_id,
                        &registered.serialize_fns,
                        &mut link,
                    );
                }
            },
        );
    }

    /// Drops every queue of `entity`'s senders, which `endpoints` holds.
    fn discard_unsent(
        endpoints: &mut FilteredEntityMut<'_, '_>,
        entity: Entity,
        message_manager: &MessageManager,
        registry: &MessageRegistry,
    ) {
        for sender in &message_manager.senders {
            let fns = registry.message(sender.net_id).send().side(sender.side);
            let dropped = (fns.discard_fn)(endpoints);
            if dropped > 0 {
                trace!(
                    ?entity,
                    net_id = ?sender.net_id,
                    dropped,
                    "dropped messages queued while not connected"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
