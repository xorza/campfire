//! Lightyear transport, handshake and replication.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod bot_driver;
mod checkpoint_thread;
mod checkpoints;
mod client_data;
mod door;
mod error;
mod events;
mod input_message;
mod join;
mod journal_watch;
mod key_file;
mod leave_match;
mod lobby;
#[cfg(feature = "internals")]
mod local_match;
mod local_pace;
mod local_relink;
mod local_server;
mod local_session;
mod match_clock;
mod match_start;
mod net_protocol;
mod offer;
mod offering;
mod order_script;
mod pace;
mod receipts;
mod save_command;
mod seats;
mod server_bots;
mod server_data;
mod server_exit;
mod server_setup;
mod server_signer;
mod session_dir;
mod session_journal;
mod session_times;
mod sim_client;
mod sim_server;
mod superseded;

pub use error::{
    JoinError, LobbyError, OrderScriptError, ReceiptRefusal, RestoreMatchError, SaveRefusal,
    TermsMismatch,
};
pub use events::avatar_missing::AvatarMissing;
pub use events::checkpoint_failed::CheckpointFailed;
pub use events::checkpoint_taken::CheckpointTaken;
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
pub use events::save_refused::SaveRefused;
pub use events::seeds_ran_out::SeedsRanOut;
pub use events::session_aborted::SessionAborted;
pub use events::session_refused::SessionRefused;
pub use events::session_restored::SessionRestored;
pub use events::session_written::SessionWritten;
pub use events::slow_sync::SlowSync;
pub use events::ticks_caught_up::TicksCaughtUp;
pub use events::time_dropped::TimeDropped;

pub use input_message::InputMessage;

pub use lobby::{Lobby, LobbySetup};
pub use local_pace::LocalPace;
pub use local_relink::LocalRelink;
pub use local_server::error::LocalServerError;
pub use local_server::{LocalServer, LocalServerSetup, Relinks};
pub use match_clock::MatchClock;
pub use pace::{Pace, Speed};
pub use save_command::SaveCommand;

pub use net_protocol::{InputChannel, NetProtocol};

pub use client_data::ClientData;
pub use journal_watch::JournalWatch;
pub use key_file::KeyFile;
pub use key_file::error::KeyFileError;
pub use order_script::{OrderScript, ScriptedInput, ScriptedOrder, ScriptedValue};
pub use server_bots::{ServerBots, SlotBot};
pub use server_data::ServerData;
pub use server_exit::ServerExit;
pub use server_setup::ServerSetup;
pub use session_dir::error::{AbortError, FindError, RestoreError, WaitingError};
pub use session_dir::snapshots::Snapshots;
pub use session_dir::{RestoredSession, SessionDir, SessionFiles};
pub use session_journal::SessionJournal;
pub use session_times::SessionTimes;
pub use sim_client::bot_script::BotScript;
pub use sim_client::join_state::{JoinState, Loss};
pub use sim_client::server_pin::ServerPin;
pub use sim_client::{LeaveRequest, PendingOrders, PendingSaves, SimClient};
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
