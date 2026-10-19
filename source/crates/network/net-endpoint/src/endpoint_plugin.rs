use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use campfire_net_link::LinkSystems;
use campfire_net_packets::DatagramIo;
use core::marker::PhantomData;

use crate::endpoint::Endpoint;
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
        app.add_observer(Endpoint::<Io>::link_start);
        app.add_observer(Endpoint::<Io>::unlink);
        app.add_systems(
            PreUpdate,
            Endpoint::<Io>::receive.in_set(LinkSystems::Receive),
        );
        app.add_systems(PostUpdate, Endpoint::<Io>::send.in_set(LinkSystems::Send));
        if !app.world().contains_resource::<PacketSessionLinkObserver>() {
            app.insert_resource(PacketSessionLinkObserver);
            app.add_observer(PacketSessionLink::unlink);
        }
    }
}

/// Marks an app whose `PacketSessionLink` observer one endpoint plugin already added, as every IO's
/// links share it.
#[derive(Resource, Debug)]
struct PacketSessionLinkObserver;
