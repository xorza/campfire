use crate::link_messages::LinkMessages;
use crate::message_error::MessageError;
use crate::message_receive_cx::MessageReceiveCx;
use crate::message_receive_cx::ReceivedMessageHeader;
use crate::message_registry::MessageModeFns;
use crate::message_registry::MessageRegistry;
use crate::message_systems::MessageSystems;
use crate::receive_map_view::ReceiveMapView;
use crate::replicon_map_param::RepliconMapParam;
use crate::send_map_view::SendMapView;
use crate::send_queue::SendLink;
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
use bevy_ecs::system::ParamBuilder;
use bevy_ecs::system::Query;
use bevy_ecs::system::QueryParamBuilder;
use bevy_ecs::system::SystemParamBuilder;
use bevy_ecs::world::FilteredEntityMut;
use bevy_ecs::world::World;
use bytes::Bytes;
use campfire_net_bytes::Reader;
use campfire_net_bytes::ToBytes;
use campfire_net_channels::ChannelMessage;
use campfire_net_channels::ChannelTypeId;
use campfire_net_channels::MessageId;
use campfire_net_connection::Client;
use campfire_net_connection::ClientOf;
use campfire_net_connection::Connected;
use campfire_net_connection::Disconnected;
use campfire_net_connection::RemoteId;
use campfire_net_core::LinkDirection;
use campfire_net_core::NetId;
use campfire_net_core::NetTick;
use campfire_net_core::PeerId;
use campfire_net_link::Unlink;
use campfire_net_payload::UnlinkReason;
use campfire_net_transport::ChannelRegistry;
use campfire_net_transport::Transport;
use campfire_net_transport::TransportSystems;
use std::sync::Arc;
use tracing::debug;
use tracing::trace;

/// Receives and sends each link's registered messages and events over its transport.
#[derive(Debug)]
pub struct MessagePlugin;

impl Plugin for MessagePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MessageRegistry>();
        app.add_observer(Self::handle_disconnection);

        app.register_required_components::<Client, LinkMessages>();

        app.register_required_components::<ClientOf, LinkMessages>();
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
            QueryParamBuilder::new(|builder| {
                builder.optional(|b| {
                    registry.messages().for_each(|message| {
                        b.mut_id(message.send().component_id);
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
struct ReceiveLink<'a> {
    entity: Entity,
    peer: PeerId,
    /// The direction every message on this link travels: toward a client or toward a server.
    arriving: LinkDirection,
    registry: &'a MessageRegistry,
    entity_map: &'a ReceiveMapView<'a>,
}

impl core::fmt::Debug for ReceiveLink<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ReceiveLink")
            .field("entity", &self.entity)
            .field("peer", &self.peer)
            .field("arriving", &self.arriving)
            .finish_non_exhaustive()
    }
}

/// One message's bytes, as a channel delivered them.
#[derive(Debug)]
struct ArrivedMessage {
    bytes: Bytes,
    channel_type_id: ChannelTypeId,
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
            if let Some(mut link_messages) = entity_mut.get_mut::<LinkMessages>() {
                link_messages.entity_mapper.clear();
            }
            entity_mut.remove_by_ids(&receive_component_ids);
        });
    }
    /// Delivers one message; an `Err` is the peer's protocol violation: bytes that do not
    /// decode, an unknown message, or a message against its registered direction.
    fn receive_message_bytes(
        link: &ReceiveLink<'_>,
        commands: &mut Commands<'_, '_>,
        receivers: &mut FilteredEntityMut<'_, '_>,
        message: ArrivedMessage,
    ) -> Result<(), MessageError> {
        let ReceiveLink {
            entity,
            peer,
            arriving,
            registry,
            entity_map,
        } = *link;
        let ArrivedMessage {
            bytes,
            channel_type_id,
            channel_name,
            tick,
            message_id,
        } = message;
        trace!(
            "Received message (id:{message_id:?}) from peer {:?} on channel {channel_type_id:?}. {entity:?}",
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
        let cx = MessageReceiveCx {
            commands,
            link: entity,
            peer,
            reader: &mut reader,
            header: ReceivedMessageHeader {
                remote_tick: tick,
                channel_type_id,
                message_id,
            },
            channel_name,
            serialize_fns: &registered.serialize_fns,
            entity_map,
        };
        match &registered.mode {
            MessageModeFns::Message { receive, .. } => (receive.receive_message_fn)(receivers, cx),
            MessageModeFns::Event { receive, .. } => (receive.receive_event_fn)(cx),
        }
    }

    /// Deserializes the messages each channel of the [`Transport`] received. A registered event
    /// triggers a [`RemoteEvent`](crate::remote_event::RemoteEvent) through `Commands`; any other
    /// message is buffered in its `MessageReceiver<M>`.
    pub(crate) fn receive(
        mut links: Query<
            '_,
            '_,
            (
                Entity,
                &LinkMessages,
                &mut Transport,
                &RemoteId,
                Has<Client>,
                // The link's receivers of every registered message.
                FilteredEntityMut<'_, '_>,
            ),
            With<Connected>,
        >,
        registry: Res<'_, MessageRegistry>,
        mut commands: Commands<'_, '_>,
        replicon: RepliconMapParam<'_, '_>,
    ) {
        let replicon = &replicon;
        links.iter_mut().for_each(
            |(entity, link_messages, mut transport, peer, is_client, mut receivers)| {
                let map = &link_messages.entity_mapper;
                let view = if is_client {
                    replicon.receive_view(map.remote_to_local())
                } else {
                    replicon.checked_view(map.local_to_remote())
                };
                let link = ReceiveLink {
                    entity,
                    peer: peer.0,
                    arriving: if is_client {
                        LinkDirection::ServerToClient
                    } else {
                        LinkDirection::ClientToServer
                    },
                    registry: &registry,
                    entity_map: &view,
                };
                for channel_receiver in transport.channel_receivers_mut() {
                    let channel_type_id = channel_receiver.channel_type_id();
                    let channel_name = channel_receiver.name();
                    while let Some(ChannelMessage {
                        tick,
                        bytes,
                        message_id,
                    }) = channel_receiver.read_message()
                    {
                        let message = ArrivedMessage {
                            bytes,
                            channel_type_id,
                            channel_name,
                            tick,
                            message_id,
                        };
                        let received = Self::receive_message_bytes(
                            &link,
                            &mut commands,
                            &mut receivers,
                            message,
                        );
                        if let Err(error) = received {
                            debug!(?entity, ?error, "unlinking a peer that broke the protocol");
                            commands.trigger(Unlink {
                                entity,
                                reason: UnlinkReason::ProtocolViolation,
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
                &LinkMessages,
                // The link's receivers of every registered message.
                FilteredEntityMut<'_, '_>,
            ),
            With<Connected>,
        >,
        registry: Res<'_, MessageRegistry>,
    ) {
        for (link_messages, mut receivers) in &mut links {
            for &net_id in &link_messages.receivers {
                let receive = registry
                    .message(net_id)
                    .receive_message()
                    .expect("only a message has a receiver");
                (receive.message_clear_fn)(&mut receivers);
            }
        }
    }
    /// Take the queued messages and events of each link's senders, serialize them, and buffer
    /// them in its [`Transport`].
    ///
    /// A link that is not connected when the frame sends, before it connects or after it
    /// disconnects, sends nothing: its queues are dropped, so that they do not outlive a packet
    /// session or grow while there is none.
    pub(crate) fn send(
        mut links: Query<
            '_,
            '_,
            (
                Entity,
                &mut Transport,
                &mut LinkMessages,
                Has<Client>,
                Has<Connected>,
                // The link's senders of every registered message.
                FilteredEntityMut<'_, '_>,
            ),
        >,
        registry: Res<'_, MessageRegistry>,
        channels: Res<'_, ChannelRegistry>,
        replicon: RepliconMapParam<'_, '_>,
    ) {
        let replicon = &replicon;
        let registry = &registry;
        let channels = &channels;
        links.iter_mut().for_each(
            |(entity, mut transport, mut link_messages, is_client, connected, mut endpoints)| {
                if !connected {
                    Self::discard_unsent(&mut endpoints, entity, &link_messages, registry);
                    return;
                }
                let LinkMessages {
                    senders,
                    writer,
                    entity_mapper,
                    ..
                } = &mut *link_messages;
                let view = if is_client {
                    replicon.send_view(entity_mapper.local_to_remote())
                } else {
                    SendMapView::AsIs
                };
                let direction = if is_client {
                    LinkDirection::ClientToServer
                } else {
                    LinkDirection::ServerToClient
                };

                // A sender that fails is skipped alone; the senders after it still send.
                for &net_id in senders.iter() {
                    let registered = registry.message(net_id);
                    let fns = registered.send();
                    if !registered.travels(direction) {
                        let dropped = (fns.discard_fn)(&mut endpoints);
                        debug_assert_eq!(
                            dropped, 0,
                            "a message is sent {direction:?}, against its registered direction"
                        );
                        continue;
                    }
                    let mut link = SendLink {
                        writer,
                        entity_map: &view,
                        channels,
                        transport: &mut transport,
                    };
                    (fns.send_fn)(&mut endpoints, net_id, &registered.serialize_fns, &mut link);
                }
            },
        );
    }

    /// Drops every queue of `entity`'s senders, which `endpoints` holds.
    fn discard_unsent(
        endpoints: &mut FilteredEntityMut<'_, '_>,
        entity: Entity,
        link_messages: &LinkMessages,
        registry: &MessageRegistry,
    ) {
        for &net_id in &link_messages.senders {
            let fns = registry.message(net_id).send();
            let dropped = (fns.discard_fn)(endpoints);
            if dropped > 0 {
                trace!(
                    ?entity,
                    ?net_id,
                    dropped,
                    "dropped messages queued while not connected"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests;
