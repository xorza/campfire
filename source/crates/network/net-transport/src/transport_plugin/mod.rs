use crate::adaptive_query_iter_mut::AdaptiveQueryIterMut;
use crate::packet_received::PacketReceived;
use crate::transport::Transport;
use crate::transport_systems::TransportSystems;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use campfire_net_channels::Channel;
use campfire_net_connection::Disconnected;
use campfire_net_connection::HostClient;
use campfire_net_connection::LinkSide;
use campfire_net_link::{Link, LinkSystems, Linked, Unlink};
use campfire_net_payload::UnlinkReason;
use campfire_net_timeline::Clock;
use campfire_net_timeline::LocalTimeline;
use tracing::{debug, trace_span, warn};

use crate::channel_registry::ChannelRegistry;

#[derive(Debug)]
pub struct TransportPlugin;

impl TransportPlugin {
    /// Reads each packet the link's packet session opened into its channels, and unlinks a peer
    /// whose packet breaks the protocol.
    fn buffer_receive(
        clock: Res<'_, Clock>,
        par_commands: ParallelCommands<'_, '_>,
        mut query: Query<
            '_,
            '_,
            (Entity, &mut Link, &mut Transport),
            (With<Linked>, Without<HostClient>),
        >,
    ) {
        let query = AdaptiveQueryIterMut::new(&mut query);
        query.for_each(|(entity, mut link, mut transport)| {
            let _span = trace_span!("transport", ?entity).entered();
            let received = transport.receive(
                &mut link.packet_session,
                clock.now(),
                clock.delta(),
                |remote_tick| {
                    par_commands.command_scope(|mut commands| {
                        commands.trigger(PacketReceived {
                            entity,
                            remote_tick,
                        });
                    });
                },
            );
            if let Err(error) = received {
                debug!(?error, "unlinking a peer that broke the protocol");
                par_commands.command_scope(|mut commands| {
                    commands.trigger(Unlink {
                        entity,
                        reason: UnlinkReason::ProtocolViolation,
                    });
                });
            }
        });
    }

    /// Packs each link's staged messages into packets for its packet session, while its budget
    /// lasts.
    fn buffer_send(
        clock: Res<'_, Clock>,
        timeline: Res<'_, LocalTimeline>,
        mut query: Query<'_, '_, (&mut Link, &mut Transport, Has<HostClient>), With<Linked>>,
    ) {
        let tick = timeline.tick();
        let query = AdaptiveQueryIterMut::new(&mut query);
        query.for_each(|(mut link, mut transport, host_client)| {
            // A host client's messages go to its buffer when they are sent, past its transport.
            if host_client {
                debug_assert!(
                    !transport.has_staged(),
                    "nothing is sent through a host client's transport"
                );
                return;
            }
            transport.flush(&mut link.packet_session, tick, clock.now());
        });
    }

    /// On disconnection, reset the Transport to its original state.
    fn handle_disconnection(
        trigger: On<'_, '_, Add<Disconnected>>,
        mut query: Query<'_, '_, &mut Transport>,
    ) {
        if let Ok(mut transport) = query.get_mut(trigger.entity) {
            transport.reset();
        }
    }

    pub(crate) fn add_sender_channel<C: Channel, R: LinkSide>(
        trigger: On<'_, '_, Insert<(Transport, R)>>,
        mut query: Query<'_, '_, &mut Transport, With<R>>,
        registry: Res<'_, ChannelRegistry>,
    ) {
        if let Ok(mut transport) = query.get_mut(trigger.entity) {
            transport.add_channel_send_from_registry::<C>(&registry);
        }
    }

    pub(crate) fn add_receiver_channel<C: Channel, R: LinkSide>(
        trigger: On<'_, '_, Insert<(Transport, R)>>,
        mut query: Query<'_, '_, &mut Transport, With<R>>,
        registry: Res<'_, ChannelRegistry>,
    ) {
        if let Ok(mut transport) = query.get_mut(trigger.entity) {
            transport.add_channel_receive_from_registry::<C>(&registry);
        }
    }
}

impl Plugin for TransportPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(Self::handle_disconnection);
        app.add_observer(Transport::on_linked);
    }

    fn finish(&self, app: &mut App) {
        if !app.world().contains_resource::<ChannelRegistry>() {
            warn!("TransportPlugin: ChannelRegistry not found, adding it");
            app.world_mut().init_resource::<ChannelRegistry>();
        }
        app.world_mut().resource_mut::<ChannelRegistry>().seal();
        app.configure_sets(
            PreUpdate,
            TransportSystems::Receive.after(LinkSystems::Receive),
        );
        app.configure_sets(PostUpdate, TransportSystems::Send.before(LinkSystems::Send));
        app.add_systems(
            PreUpdate,
            Self::buffer_receive.in_set(TransportSystems::Receive),
        );
        app.add_systems(PostUpdate, Self::buffer_send.in_set(TransportSystems::Send));
    }
}

#[cfg(test)]
mod tests;
