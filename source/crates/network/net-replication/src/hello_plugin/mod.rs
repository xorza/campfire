use crate::hello_channel::HelloChannel;
use crate::replication_sender::ReplicationSender;
use crate::replication_timer::ReplicationTimer;
use crate::sender_hello::SenderHello;
use bevy_app::prelude::*;
use bevy_ecs::prelude::*;
use campfire_net_channels::{ChannelMode, ChannelSetup};
use campfire_net_connection::Connected;
use campfire_net_core::LinkDirections;
use campfire_net_core::PositiveTickSpan;
use campfire_net_messages::LinkMessages;
use campfire_net_messages::{AppEventExt, EventSender, RemoteEvent};
use campfire_net_timeline::TimelineRate;
use campfire_net_transport::AppChannelExt;
use core::time::Duration;
use tracing::debug;

#[derive(Debug)]
pub(crate) struct HelloPlugin;

impl Plugin for HelloPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ReplicationTimer>();
        app.register_channel::<HelloChannel>(ChannelSetup {
            mode: ChannelMode::UnorderedReliable,
            send_frequency: Duration::default(),
            priority: 10.0,
            ..Default::default()
        })
        .add_direction(LinkDirections::BOTH);
        app.register_event_to_bytes::<SenderHello>()
            .add_direction(LinkDirections::BOTH);
        app.add_observer(Self::send_sender_hello);
        app.add_observer(Self::receive_sender_hello);
    }
}

impl HelloPlugin {
    /// Sends the sender's hello once a link both is connected and replicates, whichever of
    /// the two comes last: a link that never replicates gets none.
    fn send_sender_hello(
        trigger: On<'_, '_, Add<(Connected, ReplicationSender)>>,
        replication_timer: Res<'_, ReplicationTimer>,
        tick_rate: Res<'_, TimelineRate>,
        mut query: Query<
            '_,
            '_,
            (Entity, &mut EventSender<SenderHello>),
            (With<Connected>, With<ReplicationSender>),
        >,
    ) {
        let send_interval = replication_timer.timer.duration();
        let send_interval = PositiveTickSpan::from_duration(send_interval, tick_rate.0);
        if let Ok((link, mut event_sender)) = query.get_mut(trigger.entity) {
            let hello = SenderHello {
                send_interval,
                link,
            };
            event_sender.trigger::<HelloChannel>(hello);
        }
    }

    /// Maps the link entity a remote peer's hello names to the local link entity.
    fn receive_sender_hello(
        trigger: On<'_, '_, RemoteEvent<SenderHello>>,
        mut link_messages: Query<'_, '_, &mut LinkMessages>,
    ) {
        let remote_entity = trigger.event.link;
        let local_entity = trigger.link;
        if let Ok(mut messages) = link_messages.get_mut(local_entity) {
            debug!(
                "Received SenderHello: mapping remote {:?} -> local {:?}",
                remote_entity, local_entity
            );
            messages.entity_mapper.insert(remote_entity, local_entity);
        }
    }
}

#[cfg(test)]
mod tests;
