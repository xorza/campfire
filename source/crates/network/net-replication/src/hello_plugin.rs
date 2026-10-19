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
use campfire_net_core::TickRate;
use campfire_net_messages::LinkMessages;
use campfire_net_messages::{AppEventExt, EventSender, RemoteEvent};
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
        tick_rate: Res<'_, TickRate>,
        mut query: Query<
            '_,
            '_,
            (Entity, &mut EventSender<SenderHello>),
            (With<Connected>, With<ReplicationSender>),
        >,
    ) {
        let send_interval = replication_timer.timer.duration();
        let send_interval = PositiveTickSpan::from_duration(send_interval, *tick_rate);
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
mod tests {
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
                send_interval: PositiveTickSpan::lit("1"),
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
                .get_local(remote)
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

        assert_eq!(stepper.client_app(0).world().resource::<Received>().0, 1);
    }
}
