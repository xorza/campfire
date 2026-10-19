//! Client and server connection states on top of a link, and the network topology of an app
//! (13-net-stack/connection.md, One state per marker, in parallel for each layer).

#![expect(
    clippy::type_complexity,
    reason = "a Bevy query names its data and its filters in one type"
)]
#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod client;
mod client_connection_plugin;
mod client_of;
mod connect;
mod connection_plugins;
mod connection_state;
mod connection_systems;
mod disconnect;
mod host_client;
mod host_plugin;
mod link_side;
mod local_id;
mod network_target;
mod network_target_resolver;
mod network_topology;
mod network_topology_error;
mod network_topology_plugin;
mod network_topology_systems;
mod networking_state;
mod peer_set;
mod remote_id;
mod server_connection_plugin;
mod server_connection_plugins;
mod server_state;
mod start;
mod stop;

pub use crate::client::Client;
pub use crate::client_of::ClientOf;
pub use crate::connect::Connect;
pub use crate::connection_plugins::ConnectionPlugins;
pub use crate::connection_state::Connected;
pub use crate::connection_state::Connecting;
pub use crate::connection_state::Disconnected;
pub use crate::connection_state::DisconnectedReason;
pub use crate::connection_state::Disconnecting;
pub use crate::connection_systems::ConnectionSystems;
pub use crate::disconnect::Disconnect;
pub use crate::host_client::HostClient;
pub use crate::host_client::HostPayload;
pub use crate::host_plugin::HostPlugin;
pub use crate::link_side::LinkSide;
pub use crate::local_id::LocalId;
pub use crate::network_target::NetworkTarget;
pub use crate::network_target_resolver::NetworkTargetResolver;
pub use crate::network_topology::NetworkTopology;
pub use crate::network_topology_error::NetworkTopologyError;
pub use crate::network_topology_systems::NetworkTopologySystems;
pub use crate::networking_state::NetworkingState;
pub use crate::peer_set::PeerSet;
pub use crate::remote_id::RemoteId;
pub use crate::server_connection_plugins::ServerConnectionPlugins;
pub use crate::server_state::Started;
pub use crate::server_state::Starting;
pub use crate::server_state::StopReason;
pub use crate::server_state::Stopped;
pub use crate::server_state::Stopping;
pub use crate::start::Start;
pub use crate::stop::Stop;
