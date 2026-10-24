use crate::packet_received::PacketReceived;
use crate::transport::Transport;
use crate::transport_systems::TransportSystems;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use campfire_net_connection::Disconnected;
use campfire_net_connection::LinkSide;
use campfire_net_link::{Link, LinkSystems, Linked, Unlink};
use campfire_net_payload::UnlinkReason;
use campfire_net_timeline::Clock;
use campfire_net_timeline::LocalInstant;
use tracing::{debug, trace_span};

use crate::channel_registry::{ChannelGate, ChannelRegistry};
use campfire_net_channels::SendRefusal;
use campfire_net_connection::{Client, ClientOf};
use core::any::TypeId;

#[derive(Debug)]
pub struct TransportPlugin;

impl TransportPlugin {
    /// Reads each packet the link's packet session opened into its channels, announces the
    /// largest tick of those it admitted, and unlinks a peer whose packet breaks the protocol.
    fn buffer_receive(
        clock: Res<'_, Clock>,
        mut commands: Commands<'_, '_>,
        mut query: Query<'_, '_, (Entity, &mut Link, &mut Transport), With<Linked>>,
    ) {
        for (entity, mut link, mut transport) in &mut query {
            let _span = trace_span!("transport", ?entity).entered();
            match transport.receive(&mut link.packet_session, clock.now(), clock.delta()) {
                Ok(Some(remote)) => commands.trigger(PacketReceived { entity, remote }),
                Ok(None) => {}
                Err(error) => {
                    debug!(?error, "unlinking a peer that broke the protocol");
                    commands.trigger(Unlink {
                        entity,
                        reason: UnlinkReason::ProtocolViolation,
                    });
                }
            }
        }
    }

    /// Packs each link's staged messages into packets for its packet session, while its budget
    /// lasts, each stamped with the instant the timeline stands at as the frame ends; and unlinks
    /// a link whose reliable channel refused a message (11-net-stack/transport.md, A link's queued
    /// bytes have a budget).
    fn buffer_send(
        clock: Res<'_, Clock>,
        instant: LocalInstant<'_>,
        mut commands: Commands<'_, '_>,
        mut query: Query<'_, '_, (Entity, &mut Link, &mut Transport), With<Linked>>,
    ) {
        let at = instant.now();
        for (entity, mut link, mut transport) in &mut query {
            if let Some(refusal) = transport.refused() {
                debug!(
                    ?refusal,
                    "unlinking a link whose reliable channel refused a message"
                );
                let reason = match refusal {
                    SendRefusal::Backlogged { .. } => UnlinkReason::Backlogged,
                    SendRefusal::MessageTooLarge { .. } => UnlinkReason::MessageTooLarge,
                };
                commands.trigger(Unlink { entity, reason });
                continue;
            }
            transport.flush(&mut link.packet_session, at, clock.now());
        }
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

    /// Gives a link of side `R`, as its transport comes, the sender of each channel its side sends
    /// on and the receiver of each open channel it receives on; a channel the link holds keeps
    /// its state.
    fn open_channels<R: LinkSide>(
        trigger: On<'_, '_, Insert<(Transport, R)>>,
        mut query: Query<'_, '_, &mut Transport, With<R>>,
        registry: Res<'_, ChannelRegistry>,
    ) {
        let Ok(mut transport) = query.get_mut(trigger.entity) else {
            return;
        };
        for channel in registry.channels() {
            if channel.directions.contains(R::SENDS) {
                transport.open_send(channel.type_id, channel.name, channel.setup, channel.id);
            }
            if channel.directions.contains(R::RECEIVES) && channel.gate == ChannelGate::Open {
                transport.open_receive(
                    channel.type_id,
                    channel.name,
                    channel.setup,
                    channel.id,
                    channel.reader,
                );
            }
        }
    }

    /// Gives a link of side `R` the receiver of each channel `G` gates that it receives on, as it
    /// gets `G`.
    pub(crate) fn on_gate<R: LinkSide, G: Component>(
        trigger: On<'_, '_, Add<G>>,
        mut query: Query<'_, '_, &mut Transport, With<R>>,
        registry: Res<'_, ChannelRegistry>,
    ) {
        if let Ok(mut transport) = query.get_mut(trigger.entity) {
            Self::open_gated::<R, G>(&mut transport, &registry);
        }
    }

    /// Gives a link of side `R` that holds `G` already the receivers `G` gates, as its transport
    /// comes.
    pub(crate) fn on_gated_transport<R: LinkSide, G: Component>(
        trigger: On<'_, '_, Insert<(Transport, R)>>,
        mut query: Query<'_, '_, &mut Transport, (With<R>, With<G>)>,
        registry: Res<'_, ChannelRegistry>,
    ) {
        if let Ok(mut transport) = query.get_mut(trigger.entity) {
            Self::open_gated::<R, G>(&mut transport, &registry);
        }
    }

    /// Adds the receiver of each channel `G` gates that side `R` receives on, unless the link
    /// holds one: a `G` that goes and comes back keeps the channel's state.
    fn open_gated<R: LinkSide, G: Component>(
        transport: &mut Transport,
        registry: &ChannelRegistry,
    ) {
        let gate = ChannelGate::After(TypeId::of::<G>());
        for channel in registry.channels() {
            if channel.directions.contains(R::RECEIVES) && channel.gate == gate {
                transport.open_receive(
                    channel.type_id,
                    channel.name,
                    channel.setup,
                    channel.id,
                    channel.reader,
                );
            }
        }
    }
}

impl Plugin for TransportPlugin {
    fn build(&self, app: &mut App) {
        // The registry is the transport's, whichever comes first, a channel's registration or
        // the plugin.
        app.init_resource::<ChannelRegistry>();
        app.add_observer(Self::handle_disconnection);
        app.add_observer(Transport::on_linked);
        app.add_observer(Self::open_channels::<Client>);
        app.add_observer(Self::open_channels::<ClientOf>);
    }

    fn finish(&self, app: &mut App) {
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
