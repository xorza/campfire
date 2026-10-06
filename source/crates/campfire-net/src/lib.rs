//! Lightyear transport, handshake and replication.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod error;
mod events;
mod faults;
#[cfg(feature = "internals")]
mod harness;
mod input_message;
mod join;
mod leave_match;
mod local;
mod match_clock;
mod match_start;
mod net_protocol;
mod offer;
mod order_script;
mod pace;
mod save_command;
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
pub use events::journal_sync_slow::JournalSyncSlow;
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
pub use events::ticks_caught_up::TicksCaughtUp;
pub use events::time_dropped::TimeDropped;

pub use input_message::InputMessage;

pub use local::local_pace::LocalPace;
pub use local::local_relink::LocalRelink;
pub use local::local_server::error::LocalServerError;
pub use local::local_server::{LocalServer, LocalServerSetup, Relinks};
pub use match_clock::MatchClock;
pub use pace::{Pace, PaceSpeed};
pub use save_command::SaveCommand;
pub use sim_server::lobby::{Lobby, LobbySetup};

pub use net_protocol::{InputChannel, NetProtocol};

pub use order_script::{OrderScript, ScriptedInput, ScriptedOrder, ScriptedValue};
pub use session_times::SessionTimes;
pub use sim_client::bot_script::BotScript;
pub use sim_client::client_dir::ClientDir;
pub use sim_client::join_state::{JoinState, Loss};
pub use sim_client::server_pin::ServerPin;
pub use sim_client::{LeaveRequest, PendingOrders, PendingSaves, SimClient};
pub use sim_server::journal_watch::JournalWatch;
pub use sim_server::key_file::KeyFile;
pub use sim_server::key_file::error::KeyFileError;
pub use sim_server::server_bots::{ServerBots, SlotBot};
pub use sim_server::server_dir::ServerDir;
pub use sim_server::server_exit::ServerExit;
pub use sim_server::server_setup::ServerSetup;
pub use sim_server::session_dir::error::{AbortError, FindError, RestoreError, WaitingError};
pub use sim_server::session_dir::snapshot_dir::SnapshotDir;
pub use sim_server::session_dir::{RestoredSession, SessionDir, SessionFiles};
pub use sim_server::session_journal::SessionJournal;
pub use sim_server::{PlayerLink, SimServer, TickHashes};

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::sim_client::bench::{rollback, worst_client_frame};
}

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::harness::in_process_match::link_model::LinkModel;
    pub use crate::harness::in_process_match::{End, InProcessMatch, MatchSetup};
}
