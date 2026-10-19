use crate::client_connection_plugin::ClientConnectionPlugin;
use crate::client_of::ClientOf;
use crate::connection_state::Connected;
use crate::connection_state::Connecting;
use crate::local_id::LocalId;
use crate::raw_server::RawServer;
use crate::remote_id::RemoteId;
use crate::server_connection_plugin::ServerConnectionPlugin;
use crate::server_state::Started;
use crate::server_state::Starting;
use crate::server_state::Stopped;
use crate::server_state::Stopping;
use crate::stop::Stop;
use bevy_app::{App, Plugin};
use bevy_ecs::prelude::*;
use campfire_net_core::PeerId;
use campfire_net_link::PeerAddr;
use campfire_net_link::{LinkOf, Linked, Linking, Unlink};
use campfire_net_payload::UnlinkReason;
use core::fmt::Debug;
use tracing::trace;

#[derive(Debug)]
pub struct RawServerPlugin;

impl RawServerPlugin {
    /// For RawServers, Linking implies Starting
    fn on_server_linking(
        trigger: On<'_, '_, Add<Linking>>,
        query: Query<'_, '_, (), With<RawServer>>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            commands.entity(trigger.entity).try_insert(Starting);
        }
    }

    /// For RawServers, Linked implies Started
    fn on_server_linked(
        trigger: On<'_, '_, Add<Linked>>,
        query: Query<'_, '_, (), With<RawServer>>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            trace!("RawServer Linked! Adding Started");
            commands.entity(trigger.entity).try_insert(Started);
        }
    }

    /// For RawServers, a LinkOf that is Linking is Connecting
    fn on_link_of_linking(
        trigger: On<'_, '_, Add<Linking>>,
        link_of: Query<'_, '_, &LinkOf>,
        server: Query<'_, '_, (), With<RawServer>>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok(link_of) = link_of.get(trigger.entity)
            && server.contains(link_of.server)
        {
            commands.entity(trigger.entity).try_insert(Connecting);
        }
    }

    /// For RawServers, when a LinkOf gets Linked, it also becomes Connected
    ///
    /// A `RemoteId` the IO layer already put on the link is kept, as for a `RawClient`: a
    /// connection-oriented IO layer can carry several links from one `PeerAddr`, so it names
    /// each peer itself.
    fn on_link_of_linked(
        trigger: On<'_, '_, Add<Linked>>,
        link_of: Query<'_, '_, (&LinkOf, &PeerAddr, Option<&RemoteId>)>,
        server: Query<'_, '_, (), With<RawServer>>,
        mut commands: Commands<'_, '_>,
    ) {
        if let Ok((link_of, peer_addr, remote_id)) = link_of.get(trigger.entity)
            && server.contains(link_of.server)
        {
            trace!("RawServer LinkOf Linked! Adding Connected");
            commands.entity(trigger.entity).try_insert((
                Connected,
                LocalId(PeerId::Server),
                remote_id
                    .copied()
                    .unwrap_or(RemoteId(PeerId::Raw(peer_addr.0))),
                ClientOf,
            ));
        }
    }

    /// For RawServers, Stop implies Unlink, which reports the stop's code. The server's link
    /// unlinks each of its links, which the connection layer then ends.
    fn on_stop(
        trigger: On<'_, '_, Stop>,
        query: Query<'_, '_, (), (Without<Stopped>, With<RawServer>)>,
        mut commands: Commands<'_, '_>,
    ) {
        if query.contains(trigger.entity) {
            trace!("RawServer Stop! Triggering Unlink");
            // Stopping goes first: an IO layer that unlinks at once inserts Stopped, which a later
            // Stopping would remove.
            commands.entity(trigger.entity).insert(Stopping);
            commands.trigger(Unlink {
                entity: trigger.entity,
                reason: UnlinkReason::Closed(trigger.reason),
            });
        }
    }
}

impl Plugin for RawServerPlugin {
    fn build(&self, app: &mut App) {
        if !app.is_plugin_added::<ClientConnectionPlugin>() {
            app.add_plugins(ClientConnectionPlugin);
        }
        if !app.is_plugin_added::<ServerConnectionPlugin>() {
            app.add_plugins(ServerConnectionPlugin);
        }
        app.add_observer(Self::on_server_linking);
        app.add_observer(Self::on_server_linked);
        app.add_observer(Self::on_link_of_linking);
        app.add_observer(Self::on_link_of_linked);
        app.add_observer(Self::on_stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection_state::Disconnected;
    use crate::connection_state::DisconnectedReason;
    use crate::server_state::StopReason;
    use campfire_net_payload::AppCode;

    /// A `Stop` stops the server with its code, and each link of the server disconnects as its
    /// server stopped, then goes at the end of the frame.
    #[test]
    fn stop_reports_its_code_and_ends_each_link() {
        let mut app = App::new();
        app.add_plugins((campfire_net_link::ServerLinkPlugin, RawServerPlugin));
        let server = app.world_mut().spawn((RawServer, Linked)).id();
        let client = app
            .world_mut()
            .spawn((
                LinkOf { server },
                PeerAddr("127.0.0.1:5000".parse().unwrap()),
                Linked,
            ))
            .id();
        app.world_mut().flush();
        assert!(app.world().entity(server).contains::<Started>());
        assert!(app.world().entity(client).contains::<Connected>());

        let code = AppCode::new(AppCode::FIRST + 3);
        app.world_mut().trigger(Stop {
            entity: server,
            reason: code,
        });
        app.world_mut().flush();

        assert_eq!(
            app.world()
                .get::<Stopped>(server)
                .map(|stopped| stopped.reason),
            Some(StopReason::UserRequested(code))
        );
        assert_eq!(
            app.world()
                .get::<Disconnected>(client)
                .map(|disconnected| disconnected.reason),
            Some(DisconnectedReason::LinkFailed(UnlinkReason::ServerStopped))
        );
        app.update();
        assert!(app.world().get_entity(client).is_err());
        assert!(app.world().get_entity(server).is_ok());
    }

    #[test]
    fn raw_server_link_preserves_preconfigured_remote_id() {
        let mut app = App::new();
        app.add_plugins(RawServerPlugin);
        let server = app.world_mut().spawn(RawServer).id();
        let addr = "127.0.0.1:5000".parse().unwrap();
        let named = RemoteId(PeerId::Entity(7));
        let named_link = app
            .world_mut()
            .spawn((LinkOf { server }, PeerAddr(addr), named))
            .id();
        let addressed_link = app
            .world_mut()
            .spawn((LinkOf { server }, PeerAddr(addr)))
            .id();

        app.world_mut().entity_mut(named_link).insert(Linked);
        app.world_mut().entity_mut(addressed_link).insert(Linked);
        app.world_mut().flush();

        let named_link = app.world().entity(named_link);
        assert!(named_link.contains::<Connected>());
        assert_eq!(named_link.get::<RemoteId>(), Some(&named));
        let addressed_link = app.world().entity(addressed_link);
        assert!(addressed_link.contains::<Connected>());
        assert_eq!(
            addressed_link.get::<RemoteId>(),
            Some(&RemoteId(PeerId::Raw(addr)))
        );
    }
}
