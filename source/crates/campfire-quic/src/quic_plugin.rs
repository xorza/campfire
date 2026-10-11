use bevy_app::{App, Plugin, PostUpdate, PreUpdate};
use bevy_ecs::entity::Entity;
use bevy_ecs::observer::On;
use bevy_ecs::query::{With, Without};
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::{Commands, Local, Query};
use lightyear::link::server::ServerLinkPlugin;
use lightyear::link::{Link, LinkReceiveSystems, LinkStart, LinkSystems, Linked, Linking, Unlink};
use lightyear::prelude::PeerAddr;

use crate::endpoint_state::News;
use crate::quic_endpoint::QuicEndpoint;
use crate::quic_link::QuicLink;

/// An endpoint's entity that is neither linking nor linked.
type Unstarted = (Without<Linking>, Without<Linked>);

/// Links Lightyear's `Link`s over QUIC: an entity with a `QuicEndpoint` opens at its `LinkStart`;
/// in `PreUpdate`, in `BufferToLink`, each endpoint's news and received datagrams reach the
/// world, and in `PostUpdate`, in `LinkSystems::Send`, each link's packets reach their
/// connection (design 15, D3). An `Unlink` closes its connection.
#[derive(Debug)]
pub struct QuicPlugin;

impl QuicPlugin {
    fn start(
        start: On<'_, '_, LinkStart>,
        endpoints: Query<'_, '_, (&QuicEndpoint, Option<&Link>), Unstarted>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok((endpoint, link)) = endpoints.get(start.entity) {
            endpoint.start(start.entity, link, &mut commands);
        }
    }

    fn unlink(
        unlink: On<'_, '_, Unlink>,
        links: Query<'_, '_, (), With<QuicLink>>,
        endpoints: Query<'_, '_, &QuicEndpoint>,
        mut commands: Commands<'_, '_>,
    ) {
        if links.contains(unlink.entity) {
            commands.entity(unlink.entity).remove::<QuicLink>();
        }
        if let Ok(endpoint) = endpoints.get(unlink.entity) {
            endpoint.stop();
        }
    }

    fn receive(
        endpoints: Query<'_, '_, (Entity, &QuicEndpoint)>,
        mut links: Query<'_, '_, (&mut Link, &mut PeerAddr)>,
        mut commands: Commands<'_, '_>,
        mut news: Local<'_, Vec<News>>,
    ) {
        for (entity, endpoint) in &endpoints {
            endpoint.receive(entity, &mut links, &mut commands, &mut news);
        }
    }

    fn send(
        endpoints: Query<'_, '_, &QuicEndpoint>,
        mut links: Query<'_, '_, &mut Link, With<Linked>>,
    ) {
        for endpoint in &endpoints {
            endpoint.send(&mut links);
        }
    }
}

impl Plugin for QuicPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ServerLinkPlugin>() {
            app.add_plugins(ServerLinkPlugin);
        }
        app.add_observer(QuicPlugin::start);
        app.add_observer(QuicPlugin::unlink);
        app.add_systems(
            PreUpdate,
            QuicPlugin::receive.in_set(LinkReceiveSystems::BufferToLink),
        );
        app.add_systems(PostUpdate, QuicPlugin::send.in_set(LinkSystems::Send));
    }
}
