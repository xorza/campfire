use core::marker::PhantomData;

use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::schedule::IntoScheduleConfigs;
use campfire_net_link::LinkSystems;
use campfire_net_packets::DatagramIo;

use crate::endpoint::Endpoint;
use crate::frame_wake::FrameWake;
use crate::packet_session_link::PacketSessionLink;

/// The systems of the endpoints of one IO: the frame's two steps and their lifecycle.
#[derive(Debug)]
pub struct EndpointPlugin<Io>(PhantomData<Io>);

impl<Io> Default for EndpointPlugin<Io> {
    fn default() -> Self {
        Self(PhantomData)
    }
}

impl<Io: DatagramIo> Plugin for EndpointPlugin<Io> {
    fn build(&self, app: &mut App) {
        app.init_resource::<FrameWake>();
        app.add_observer(Endpoint::<Io>::link_start);
        app.add_observer(Endpoint::<Io>::unlink);
        app.add_systems(
            PreUpdate,
            Endpoint::<Io>::receive.in_set(LinkSystems::Receive),
        );
        app.add_systems(PostUpdate, Endpoint::<Io>::send.in_set(LinkSystems::Send));
    }
}

/// The `PacketSessionLink` observer, which every IO's links share: an app of any endpoint plugin
/// lists it once (11-net-stack/api.md, One plugin group for each kind of app, and no plugin adds
/// another).
#[derive(Debug)]
pub struct PacketSessionLinkPlugin;

impl Plugin for PacketSessionLinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(PacketSessionLink::unlink);
    }
}
