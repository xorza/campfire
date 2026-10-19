use crate::server::Server;
use bevy_app::{App, Plugin};

/// Plugin that installs server/link relationship support.
///
/// On [`LinkPlugin`](crate::LinkPlugin), which the app adds, it adds the observer that reacts to
/// [`Unlinked`](crate::link_state::Unlinked) on server entities by unlinking their child links.
/// Transport crates that expose a multi-client server endpoint should add this plugin before
/// scheduling their server IO systems.
#[derive(Debug)]
pub struct ServerLinkPlugin;

impl Plugin for ServerLinkPlugin {
    fn build(&self, app: &mut App) {
        app.add_observer(Server::unlinked);
    }
}
