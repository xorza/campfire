//! Lightyear transport, handshake and replication.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod error;
mod input_message;
mod join;
mod lobby;
#[cfg(feature = "internals")]
mod local_match;
mod match_clock;
mod match_start;
mod net_protocol;
mod offer;
mod order_script;
mod sim_client;
mod sim_server;

pub use error::{JoinError, OrderScriptError, TermsMismatch};
pub use input_message::InputMessage;
pub use join::Join;
pub use lobby::{JoinRefused, Joined, Lobby, LobbySetup};
#[cfg(feature = "internals")]
pub use local_match::link_model::LinkModel;
#[cfg(feature = "internals")]
pub use local_match::{LocalMatch, MatchSetup};
pub use match_clock::MatchClock;
pub use match_start::MatchStart;
pub use net_protocol::{InputChannel, JoinChannel, MatchChannel, NetProtocol};
pub use offer::Offer;
pub use order_script::OrderScript;
pub use sim_client::client_mode::ClientMode;
pub use sim_client::server_pin::ServerPin;
pub use sim_client::unpredicted::Unpredicted;
pub use sim_client::{JoinState, PendingOrders, SimClient};
pub use sim_server::{PlayerLink, SimServer, TickHashes};

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::sim_client::bench::rollback;
}
