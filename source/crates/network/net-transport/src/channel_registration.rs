use crate::channel_registry::ChannelRegistry;
use crate::transport::Transport;
use bevy_app::App;
use bevy_ecs::prelude::*;
use campfire_net_channels::Channel;
use campfire_net_connection::Client;
use campfire_net_connection::ClientOf;
use campfire_net_connection::LinkRole;
use campfire_net_core::Directions;

#[derive(Debug)]
pub struct ChannelRegistration<'a, C> {
    pub app: &'a mut App,
    pub(crate) _marker: core::marker::PhantomData<C>,
}

impl<C: Channel> ChannelRegistration<'_, C> {
    /// Widens the directions the channel travels in.
    pub fn add_direction(&mut self, directions: impl Into<Directions>) -> &mut Self {
        let directions = directions.into();
        self.app
            .world_mut()
            .resource_mut::<ChannelRegistry>()
            .add_direction::<C>(directions);
        self.add_role_direction::<Client>(directions);
        self.add_role_direction::<ClientOf>(directions);
        self
    }

    /// Gives each link of role `R` the channel's sender or receiver, as `directions` have it send
    /// or receive.
    fn add_role_direction<R: LinkRole>(&mut self, directions: Directions) {
        if directions.contains(R::SENDS) {
            self.app.add_observer(add_sender_channel::<C, R>);
        }
        if directions.contains(R::RECEIVES) {
            self.app.add_observer(add_receiver_channel::<C, R>);
        }
    }
}

fn add_sender_channel<C: Channel, R: LinkRole>(
    trigger: On<'_, '_, Insert<(Transport, R)>>,
    mut query: Query<'_, '_, &mut Transport, With<R>>,
    registry: Res<'_, ChannelRegistry>,
) {
    if let Ok(mut transport) = query.get_mut(trigger.entity) {
        transport.add_channel_send_from_registry::<C>(&registry);
    }
}

fn add_receiver_channel<C: Channel, R: LinkRole>(
    trigger: On<'_, '_, Insert<(Transport, R)>>,
    mut query: Query<'_, '_, &mut Transport, With<R>>,
    registry: Res<'_, ChannelRegistry>,
) {
    if let Ok(mut transport) = query.get_mut(trigger.entity) {
        transport.add_channel_receive_from_registry::<C>(&registry);
    }
}
