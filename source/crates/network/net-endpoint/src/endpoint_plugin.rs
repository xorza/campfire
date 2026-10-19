use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use campfire_net_link::{LinkPlugin, LinkSystems};
use campfire_net_packets::DatagramIo;
use core::marker::PhantomData;

use crate::endpoint::Endpoint;
use crate::session_link::SessionLink;

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
        if !app.is_plugin_added::<LinkPlugin>() {
            app.add_plugins(LinkPlugin);
        }
        app.add_observer(Endpoint::<Io>::link_start);
        app.add_observer(Endpoint::<Io>::unlink);
        app.add_systems(
            PreUpdate,
            Endpoint::<Io>::receive.in_set(LinkSystems::Receive),
        );
        app.add_systems(PostUpdate, Endpoint::<Io>::send.in_set(LinkSystems::Send));
        if !app.world().contains_resource::<SessionLinkObserver>() {
            app.insert_resource(SessionLinkObserver);
            app.add_observer(SessionLink::unlink);
        }
    }
}

/// Marks an app whose `SessionLink` observer one endpoint plugin already added, as every IO's
/// links share it.
#[derive(Resource, Debug)]
struct SessionLinkObserver;
