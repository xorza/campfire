//! Lightyear transport, handshake and replication.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod input_message;
#[cfg(feature = "internals")]
mod local_pair;
mod match_clock;
mod match_start;
mod net_protocol;
mod sim_client;
mod sim_server;

pub use input_message::InputMessage;
#[cfg(feature = "internals")]
pub use local_pair::LocalPair;
pub use match_clock::MatchClock;
pub use match_start::MatchStart;
pub use net_protocol::{InputChannel, MatchChannel, NetProtocol};
pub use sim_client::unpredicted::Unpredicted;
pub use sim_client::{PendingOrders, SimClient};
pub use sim_server::{PlayerLink, SimServer, TickHashes};

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::sim_client::bench::rollback;
}
