//! Lightyear transport, handshake and replication.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod bot_driver;
mod door;
mod error;
mod events;
mod input_message;
mod join;
mod leave_match;
mod lobby;
#[cfg(feature = "internals")]
mod local_match;
mod match_clock;
mod match_start;
mod net_protocol;
mod offer;
mod offering;
mod order_script;
mod receipts;
mod seats;
mod server_bots;
mod server_setup;
mod server_signer;
mod session_dir;
mod session_journal;
mod session_times;
mod sim_client;
mod sim_server;
mod superseded;

pub use error::{JoinError, OrderScriptError, ReceiptRefusal, TermsMismatch};
pub use events::avatar_missing::AvatarMissing;
pub use events::input_dropped::InputDropped;
pub use events::input_logged::InputLogged;
pub use events::input_message_refused::InputMessageRefused;
pub use events::input_message_unfit::InputMessageUnfit;
pub use events::input_never_applied::{InputNeverApplied, Unapplied};
pub use events::inputs_discarded::InputsDiscarded;
pub use events::join_refused::JoinRefused;
pub use events::journal_failed::JournalFailed;
pub use events::link_lost::LinkLost;
pub use events::listening::Listening;
pub use events::match_started::MatchStarted;
pub use events::order_dropped::OrderDropped;
pub use events::orders_sent::OrdersSent;
pub use events::receipt_refused::ReceiptRefused;
pub use events::receipt_unsaved::ReceiptUnsaved;
pub use events::session_aborted::SessionAborted;
pub use events::session_refused::SessionRefused;
pub use events::session_restored::SessionRestored;
pub use events::session_written::SessionWritten;
pub use events::ticks_caught_up::TicksCaughtUp;
pub use events::time_dropped::TimeDropped;

pub use input_message::InputMessage;

pub use lobby::{Lobby, LobbySetup};
pub use match_clock::MatchClock;

pub use net_protocol::{InputChannel, NetProtocol};

pub use order_script::{OrderScript, ScriptedInput, ScriptedOrder, ScriptedValue};
pub use server_bots::{ServerBots, SlotBot};
pub use server_setup::ServerSetup;
pub use session_dir::error::{AbortError, FindError, RestoreError};
pub use session_dir::{RestoredSession, SessionDir};
pub use session_journal::SessionJournal;
pub use session_times::SessionTimes;
pub use sim_client::bot_script::BotScript;
pub use sim_client::join_state::{JoinState, Loss};
pub use sim_client::server_pin::ServerPin;
pub use sim_client::{LeaveRequest, PendingOrders, SimClient};
pub use sim_server::{PlayerLink, SimServer, TickHashes};

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::sim_client::bench::{rollback, worst_client_frame};
}

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::local_match::link_model::LinkModel;
    pub use crate::local_match::{End, LocalMatch, MatchSetup};
}
