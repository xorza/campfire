use bevy_app::{App, Plugin};
use bevy_reflect::Reflect;
use campfire_net_channels::Channel;
use campfire_net_channels::ChannelMode;
use campfire_net_channels::ChannelSetup;
use campfire_net_channels::ChannelTypeId;
use campfire_net_core::LinkDirections;
use campfire_net_core::NetId;

use crate::app_channel_ext::AppChannelExt;
use crate::channel_registry::ChannelRegistry;

#[derive(Debug, Reflect)]
pub struct Channel1;
impl Channel for Channel1 {}

#[derive(Debug, Reflect)]
pub struct ChannelSequencedUnreliable;
impl Channel for ChannelSequencedUnreliable {}

#[derive(Debug, Reflect)]
pub struct ChannelUnorderedReliable;
impl Channel for ChannelUnorderedReliable {}

#[derive(Debug, Reflect)]
pub struct ChannelSequencedReliable;
impl Channel for ChannelSequencedReliable {}

#[derive(Debug, Reflect)]
pub struct ChannelOrderedReliable;
impl Channel for ChannelOrderedReliable {}

/// The net id `app`'s registry gives the channel `C`.
pub fn channel_id<C: Channel>(app: &App) -> NetId {
    app.world()
        .resource::<ChannelRegistry>()
        .net_id(ChannelTypeId::of::<C>())
        .unwrap()
}

/// The transport layer's test protocol: one channel of each mode, both ways.
#[derive(Debug)]
pub struct TransportProtocolPlugin;

impl Plugin for TransportProtocolPlugin {
    fn build(&self, app: &mut App) {
        let channel = |mode| ChannelSetup {
            mode,
            ..ChannelSetup::default()
        };
        app.register_channel::<Channel1>(channel(ChannelMode::UnorderedUnreliable))
            .add_direction(LinkDirections::BOTH);
        app.register_channel::<ChannelSequencedUnreliable>(channel(
            ChannelMode::SequencedUnreliable,
        ))
        .add_direction(LinkDirections::BOTH);
        app.register_channel::<ChannelUnorderedReliable>(channel(ChannelMode::UnorderedReliable))
            .add_direction(LinkDirections::BOTH);
        app.register_channel::<ChannelSequencedReliable>(channel(ChannelMode::SequencedReliable))
            .add_direction(LinkDirections::BOTH);
        app.register_channel::<ChannelOrderedReliable>(channel(ChannelMode::OrderedReliable))
            .add_direction(LinkDirections::BOTH);
    }
}
