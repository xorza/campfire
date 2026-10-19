use crate::network_topology_plugin;
use bevy_app::{App, Plugin};

/// Plugin to handle the connection logic.
/// Registers relevant types.
#[derive(Debug)]
pub struct ConnectionPlugin;

impl Plugin for ConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(network_topology_plugin::NetworkTopologyPlugin);
    }
}
