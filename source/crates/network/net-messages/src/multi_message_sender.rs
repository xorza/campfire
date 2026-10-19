use crate::endpoint_side::EndpointSide;
use crate::message::Message;
use crate::message_error::MessageError;
use crate::message_kind::MessageKind;
use crate::message_manager::MessageManager;
use crate::message_registry::MessageRegistry;
use crate::replicon_map_param::RepliconMapParam;
use crate::send_map_view::SendMapView;
use crate::send_queue::Priority;
use crate::send_queue::SendTarget;
use bevy_ecs::query::{Has, QueryFilter, With};
use bevy_ecs::{
    entity::EntitySet,
    relationship::RelationshipTarget,
    system::{Local, Query, Res, SystemParam},
};
use bytes::Bytes;
use campfire_common::ErrorReport;
use campfire_net_bytes::Writer;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelKind;
use campfire_net_channels::TransportError;
use campfire_net_connection::Client;
use campfire_net_connection::Connected;
use campfire_net_connection::HostClient;
use campfire_net_connection::NetworkTarget;
use campfire_net_connection::NetworkTargetResolver;
use campfire_net_connection::NetworkingState;
use campfire_net_core::NetId;
use campfire_net_link::Server;
use campfire_net_timeline::LocalTimeline;
use campfire_net_transport::ChannelRegistry;
use campfire_net_transport::Transport;

/// SystemParam that sends one message or event to several links at once, serializing it once
/// when it maps no entities.
///
/// It addresses a set of links, or the clients of a [`Server`] that a [`NetworkTarget`] names. A
/// registered event arrives as a [`RemoteEvent`](crate::remote_event::RemoteEvent), as an
/// [`EventSender`](crate::event_sender::EventSender)'s does.
///
/// Each link sends from its own side, as its
/// [`MessageSender`](crate::message_sender::MessageSender) does: a host client is the server's link
/// to it, which sends toward the host client through its buffer, and the host client's own systems
/// send to their server through a
/// [`HostMessageSender`](crate::host_message_sender::HostMessageSender). A message sent against its
/// registered direction is a bug: a debug build panics, and a release build drops it, as a peer
/// would unlink a sender that broke the protocol.
///
/// Only connected links receive. Entities map as a link's own sends map them: on a client
/// only, through replicon's map, then the link's own pairs.
#[derive(SystemParam)]
pub struct MultiMessageSender<'w, 's, F: QueryFilter + 'static = ()> {
    query: Query<
        'w,
        's,
        (
            &'static MessageManager,
            &'static mut Transport,
            Option<&'static mut HostClient>,
            Has<Client>,
        ),
        (F, With<Connected>),
    >,
    replicon: RepliconMapParam<'w, 's>,
    registry: Res<'w, MessageRegistry>,
    channels: Res<'w, ChannelRegistry>,
    timeline: Res<'w, LocalTimeline>,
    networking: Res<'w, NetworkingState>,
    resolver: Local<'s, NetworkTargetResolver>,
    writer: Local<'s, Writer>,
}

/// A query filter is not `Debug`, so the filter takes no part in it.
impl<F: QueryFilter> core::fmt::Debug for MultiMessageSender<'_, '_, F> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MultiMessageSender")
            .field("registry", &self.registry)
            .field("networking", &self.networking)
            .finish_non_exhaustive()
    }
}

impl<F: QueryFilter> MultiMessageSender<'_, '_, F> {
    /// Sends `message` to each of `links` that is connected.
    pub fn send<M: Message, C: Channel>(
        &mut self,
        message: &M,
        links: impl EntitySet,
    ) -> Result<(), MessageError> {
        self.send_with_priority::<M, C>(message, links, 1.0)
    }

    /// Sends `message` to each of `links` that is connected, at a bandwidth `priority`.
    pub fn send_with_priority<M: Message, C: Channel>(
        &mut self,
        message: &M,
        links: impl EntitySet,
        priority: Priority,
    ) -> Result<(), MessageError> {
        let channel = self
            .channels
            .net_id(ChannelKind::of::<C>())
            .expect("a message is sent on a registered channel");
        let tick = self.timeline.tick();
        let registered = self.registry.message(
            self.registry
                .net_id(MessageKind::of::<M>())
                .expect("a message is registered before it is sent"),
        );
        let maps_entities = registered.serialize_fns.maps_entities;
        let travels = |direction| registered.travels(direction);
        // The bytes as this world names its entities, serialized once for every link that
        // takes them so: each link of a server, and a host client.
        let mut as_is: Option<Bytes> = None;
        for (manager, mut transport, mut host_client, is_client) in
            self.query.iter_many_unique_mut(links).matched()
        {
            let remote_client = is_client && host_client.is_none();
            let sends = EndpointSide::Link.sends(remote_client);
            debug_assert!(
                travels(sends),
                "{} is sent {sends:?}, against its registered direction: on a host client, the \
                 client's own systems send through its host sender",
                core::any::type_name::<M>()
            );
            if !travels(sends) {
                continue;
            }
            let bytes = if maps_entities && remote_client {
                let view = self
                    .replicon
                    .send_view(manager.entity_mapper.local_to_remote());
                self.registry
                    .serialize::<M>(message, &mut self.writer, &view)?;
                self.writer.take_written()
            } else if let Some(bytes) = &as_is {
                bytes.clone()
            } else {
                self.registry
                    .serialize::<M>(message, &mut self.writer, &SendMapView::AsIs)?;
                as_is.insert(self.writer.take_written()).clone()
            };
            let target = match host_client.as_deref_mut() {
                Some(host_client) => SendTarget::Host {
                    buffer: &mut host_client.buffer,
                    tick,
                    direction: sends,
                },
                None => SendTarget::Transport(&mut transport),
            };
            send(target, channel, bytes, priority)?;
        }
        Ok(())
    }

    /// Sends `message` to the connected clients of `server` that `target` names.
    pub fn send_to_target<M: Message, C: Channel>(
        &mut self,
        message: &M,
        server: &Server,
        target: &NetworkTarget,
    ) -> Result<(), MessageError> {
        self.send_to_target_with_priority::<M, C>(message, server, target, 1.0)
    }

    /// Sends `message` to the connected clients of `server` that `target` names, at a bandwidth
    /// `priority`.
    pub fn send_to_target_with_priority<M: Message, C: Channel>(
        &mut self,
        message: &M,
        server: &Server,
        target: &NetworkTarget,
        priority: Priority,
    ) -> Result<(), MessageError> {
        let mut resolver = core::mem::take(&mut *self.resolver);
        let links = resolver.resolve(
            target,
            server.collection().as_slice(),
            &self.networking.peer_map,
        );
        let sent = self.send_with_priority::<M, C>(message, links, priority);
        *self.resolver = resolver;
        sent
    }
}

/// A message too large for its channel is the caller's to handle; any other transport error is a
/// message sent on a channel its transport does not send on.
fn send(
    mut target: SendTarget<'_>,
    channel: NetId,
    bytes: Bytes,
    priority: Priority,
) -> Result<(), MessageError> {
    match target.send(channel, bytes, priority) {
        Ok(()) => Ok(()),
        Err(error @ TransportError::MessageTooLarge { .. }) => Err(error.into()),
        Err(error) => {
            panic!(
                "a message is sent only on a channel its transport sends on: {}",
                ErrorReport::of(&error)
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::internals::host_received_messages::HostReceivedMessages;
    use crate::internals::messages_stack::MessagesStack;
    use crate::internals::protocol::{ClientMessage, StringEvent, StringMessage};
    use crate::internals::received_messages::ReceivedMessages;
    use crate::remote_event::RemoteEvent;
    use bevy_ecs::entity::EntityIndexSet;
    use bevy_ecs::entity::{Entity, UniqueEntityArray};
    use bevy_ecs::observer::On;
    use bevy_ecs::prelude::Single;
    use bevy_ecs::resource::Resource;
    use bevy_ecs::system::ResMut;
    use campfire_net_connection::Disconnecting;
    use campfire_net_connection::RemoteId;
    use campfire_net_core::PeerId;
    use campfire_net_payload::AppCode;
    use campfire_net_stepper::Stepper;
    use campfire_net_stepper::StepperConfig;
    use campfire_net_transport::internals::protocol::Channel1;

    fn received(stepper: &Stepper<MessagesStack>, client: usize) -> &[(Entity, StringMessage)] {
        &stepper
            .client_app(client)
            .world()
            .resource::<ReceivedMessages<StringMessage>>()
            .0
    }

    /// A message to two links reaches each of their clients once.
    #[test]
    fn a_message_reaches_each_listed_link() {
        let mut stepper = Stepper::new(MessagesStack, StepperConfig::with_raw_clients(2));
        for client in 0..2 {
            ReceivedMessages::<StringMessage>::collect_in(stepper.client_app_mut(client));
        }
        let links = [stepper.client_of_entity(0), stepper.client_of_entity(1)];
        let message = StringMessage("World".to_string());
        let sent = message.clone();
        let system = stepper.server_app_mut().register_system(
            move |mut sender: MultiMessageSender<'_, '_>| {
                let links = EntityIndexSet::from_iter(links);
                sender.send::<_, Channel1>(&sent, &links).unwrap();
            },
        );
        stepper
            .server_app_mut()
            .world_mut()
            .run_system(system)
            .unwrap();
        stepper.frame_step(2);
        for client in 0..2 {
            let expected = [(stepper.client_entity(client), message.clone())];
            assert_eq!(received(&stepper, client), &expected, "client {client}");
        }
    }

    #[derive(Resource, Default)]
    struct Triggered(Vec<(PeerId, StringEvent)>);

    /// An event arrives as a remote event, once on each listed link, from the server.
    #[test]
    fn an_event_reaches_each_listed_link() {
        let mut stepper = Stepper::new(MessagesStack, StepperConfig::with_raw_clients(2));
        for client in 0..2 {
            let app = stepper.client_app_mut(client);
            app.init_resource::<Triggered>();
            app.add_observer(
                |event: On<'_, '_, RemoteEvent<StringEvent>>,
                 mut triggered: ResMut<'_, Triggered>| {
                    triggered.0.push((event.from, event.event.clone()));
                },
            );
        }
        let links = [stepper.client_of_entity(0), stepper.client_of_entity(1)];
        let event = StringEvent("Event".to_string());
        let sent = event.clone();
        let system = stepper.server_app_mut().register_system(
            move |mut sender: MultiMessageSender<'_, '_>| {
                let links = EntityIndexSet::from_iter(links);
                sender.send::<_, Channel1>(&sent, &links).unwrap();
            },
        );
        stepper
            .server_app_mut()
            .world_mut()
            .run_system(system)
            .unwrap();
        stepper.frame_step(2);
        for client in 0..2 {
            assert_eq!(
                stepper.client_app(client).world().resource::<Triggered>().0,
                [(PeerId::Server, event.clone())],
                "client {client}"
            );
        }
    }

    /// `NetworkTarget::All` reaches both clients; `Single` then reaches only its own, as the
    /// sender's reused target set keeps nothing of the broadcast before it.
    #[test]
    fn a_target_selects_its_clients() {
        let mut stepper = Stepper::new(MessagesStack, StepperConfig::with_raw_clients(2));
        for client in 0..2 {
            ReceivedMessages::<StringMessage>::collect_in(stepper.client_app_mut(client));
        }
        let message = StringMessage("World".to_string());
        let sent = message.clone();
        let client_0 = stepper.client_of(0).get::<RemoteId>().unwrap().0;
        let system = stepper.server_app_mut().register_system(
            move |mut sender: MultiMessageSender<'_, '_>,
                  server: Single<'_, '_, &Server>,
                  mut sent_all: Local<'_, bool>| {
                let target = if *sent_all {
                    NetworkTarget::single(client_0)
                } else {
                    NetworkTarget::All
                };
                sender
                    .send_to_target::<_, Channel1>(&sent, server.into_inner(), &target)
                    .unwrap();
                *sent_all = true;
            },
        );
        stepper
            .server_app_mut()
            .world_mut()
            .run_system(system)
            .unwrap();
        stepper.frame_step(2);
        for client in 0..2 {
            let expected = [(stepper.client_entity(client), message.clone())];
            assert_eq!(received(&stepper, client), &expected, "client {client}");
        }

        for client in 0..2 {
            stepper
                .client_app_mut(client)
                .world_mut()
                .resource_mut::<ReceivedMessages<StringMessage>>()
                .0
                .clear();
        }
        stepper
            .server_app_mut()
            .world_mut()
            .run_system(system)
            .unwrap();
        stepper.frame_step(2);
        let expected = [(stepper.client_entity(0), message)];
        assert_eq!(received(&stepper, 0), &expected);
        assert_eq!(received(&stepper, 1), &[]);
    }

    /// `NetworkTarget::All` skips a link that is not connected: here client 1's link is
    /// disconnecting, and only client 0 receives.
    #[test]
    fn all_reaches_only_connected_links() {
        let mut stepper = Stepper::new(MessagesStack, StepperConfig::with_raw_clients(2));
        for client in 0..2 {
            ReceivedMessages::<StringMessage>::collect_in(stepper.client_app_mut(client));
        }
        stepper.client_of_mut(1).insert(Disconnecting {
            reason: AppCode::ENDED,
        });
        let message = StringMessage("World".to_string());
        let sent = message.clone();
        let system = stepper.server_app_mut().register_system(
            move |mut sender: MultiMessageSender<'_, '_>, server: Single<'_, '_, &Server>| {
                sender
                    .send_to_target::<_, Channel1>(&sent, server.into_inner(), &NetworkTarget::All)
                    .unwrap();
            },
        );
        stepper
            .server_app_mut()
            .world_mut()
            .run_system(system)
            .unwrap();
        stepper.frame_step(2);
        assert_eq!(
            received(&stepper, 0),
            &[(stepper.client_entity(0), message)]
        );
        assert_eq!(received(&stepper, 1), &[]);
    }

    /// A host client receives what the server sends to all, a frame after a remote client does,
    /// and alone what it sends to the host's id.
    #[test]
    fn a_host_client_receives_its_targets() {
        let mut stepper = Stepper::new(MessagesStack, StepperConfig::host_server());
        let host = stepper.host_client_entity();
        let host_id = stepper.host_client().get::<RemoteId>().unwrap().0;
        HostReceivedMessages::<StringMessage>::collect_in(stepper.server_app_mut());
        ReceivedMessages::<StringMessage>::collect_in(stepper.client_app_mut(0));
        let send = |stepper: &mut Stepper<MessagesStack>, target: NetworkTarget, text: &str| {
            let message = StringMessage(text.to_string());
            let system = stepper.server_app_mut().register_system(
                move |mut sender: MultiMessageSender<'_, '_>, server: Single<'_, '_, &Server>| {
                    sender
                        .send_to_target::<_, Channel1>(&message, server.into_inner(), &target)
                        .unwrap();
                },
            );
            stepper
                .server_app_mut()
                .world_mut()
                .run_system(system)
                .unwrap();
        };
        let host_received = |stepper: &mut Stepper<MessagesStack>| {
            core::mem::take(
                &mut stepper
                    .server_app_mut()
                    .world_mut()
                    .resource_mut::<HostReceivedMessages<StringMessage>>()
                    .0,
            )
        };

        send(&mut stepper, NetworkTarget::All, "All");
        stepper.frame_step(2);
        let all = StringMessage("All".to_string());
        assert_eq!(
            received(&stepper, 0),
            &[(stepper.client_entity(0), all.clone())]
        );
        assert_eq!(host_received(&mut stepper), [(host, all)]);

        send(&mut stepper, NetworkTarget::single(host_id), "Host");
        stepper.frame_step(2);
        assert_eq!(
            received(&stepper, 0).len(),
            1,
            "the remote client gets nothing more"
        );
        assert_eq!(
            host_received(&mut stepper),
            [(host, StringMessage("Host".to_string()))]
        );
    }

    /// A message only a client sends, sent to a host client, would go from the server's link to
    /// it, against its direction: the host client's own systems send through its host sender.
    #[test]
    #[should_panic(expected = "is sent ServerToClient, against its registered direction")]
    fn a_message_against_its_direction_is_a_bug() {
        let mut stepper = Stepper::new(MessagesStack, StepperConfig::host_server());
        let host = stepper.host_client_entity();
        let system = stepper.server_app_mut().register_system(
            move |mut sender: MultiMessageSender<'_, '_>| {
                let message = ClientMessage("Host".to_string());
                sender
                    .send::<_, Channel1>(&message, UniqueEntityArray::from([host]))
                    .unwrap();
            },
        );
        stepper
            .server_app_mut()
            .world_mut()
            .run_system(system)
            .unwrap();
    }
}
