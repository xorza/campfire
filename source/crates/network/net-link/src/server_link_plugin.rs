use crate::server::Server;
use bevy_app::{App, Plugin};

/// A server's links: an unlinked server unlinks each of its links. An IO crate with a server
/// endpoint adds it before it schedules its server's IO.
#[derive(Debug)]
pub struct ServerLinkPlugin;

impl Plugin for ServerLinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(Server::unlinked);
    }
}
