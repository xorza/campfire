//! A session over the network stack: its messages on the stack's channels, the server's and the
//! client's plugins on their links, and the local server of a singleplayer client.

mod close_code;
mod events;
mod local;
mod local_pace;
mod net_protocol;
mod paced_runner;
#[cfg(feature = "internals")]
mod protocol_client;
mod sim_client;
mod sim_server;
mod udp_links;

pub use crate::close_code::CloseCode;
pub use crate::events::link_lost::{LinkLossCause, LinkLost};
pub use crate::local::local_link::LocalLink;
pub use crate::local::local_server::error::LocalServerError;
pub use crate::local::local_server::{
    LocalLobby, LocalReplay, LocalServer, LocalServerSetup, LocalSession,
};
pub use crate::local_pace::LocalPacePlugin;
pub use crate::net_protocol::{InputChannel, NetProtocolPlugin};
pub use crate::paced_runner::PacedRunnerPlugin;
pub use crate::sim_client::client_console::ClientConsole;
pub use crate::sim_client::{LeaveRequest, PendingJournalCommands, PendingOrders, SimClientPlugin};
pub use crate::sim_server::lobby::error::{KeepFilesError, LobbyError, LobbyOpenError};
pub use crate::sim_server::lobby::{Lobby, LobbyOpening, LobbySetup, LobbyStart};
pub use crate::sim_server::pending_replay::PendingReplay;
pub use crate::sim_server::pending_restore::{PendingRestore, UncheckedRestore};
pub use crate::sim_server::server_app_setup::{ServerAppSetup, ServerRun};
pub use crate::sim_server::sim_server_plugins::SimServerPlugins;
pub use crate::sim_server::sim_server_world::{SessionOpening, SimServerWorld, StopRequest};
pub use crate::udp_links::UdpLinks;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::protocol_client::{ProtocolClient, ProtocolClientState};
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use crate::sim_server::sim_server_world::internals::SimServerWorldInternals;
}
