//! Lightyear transport, handshake and replication.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod error;
mod events;
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
pub use events::input_logged::InputLogged;
pub use events::link_lost::LinkLost;
pub use events::listening::Listening;
pub use events::match_started::MatchStarted;
pub use events::orders_sent::OrdersSent;
pub use events::session_written::SessionWritten;

pub use input_message::InputMessage;

pub use lobby::{Lobby, LobbySetup};
#[cfg(feature = "internals")]
pub use local_match::link_model::LinkModel;
#[cfg(feature = "internals")]
pub use local_match::{LocalMatch, MatchSetup};
pub use match_clock::MatchClock;

pub use net_protocol::{InputChannel, NetProtocol};

pub use order_script::OrderScript;
pub use sim_client::bot_script::BotScript;
pub use sim_client::join_state::JoinState;
pub use sim_client::server_pin::ServerPin;
pub use sim_client::{PendingOrders, SimClient};
pub use sim_server::{PlayerLink, SimServer, TickHashes};

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::sim_client::bench::{rollback, worst_client_frame};
}
