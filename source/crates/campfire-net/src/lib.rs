//! Lightyear transport, handshake and replication.

#![allow(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

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

pub use crate::sim_server::lobby::error::LobbyError;

pub use crate::sim_server::checkpoints::error::SaveRefusal;

pub use crate::sim_client::join_state::error::ReceiptRefusal;

pub use crate::sim_client::join_state::error::TermsMismatch;

pub use crate::order_script::error::OrderScriptError;

pub use crate::sim_server::error::JoinError;

pub use crate::events::avatar_missing::AvatarMissing;
pub use crate::events::checkpoint_failed::CheckpointFailed;
pub use crate::events::checkpoint_taken::CheckpointTaken;
pub use crate::events::input_dropped::InputDropped;
pub use crate::events::input_logged::InputLogged;
pub use crate::events::input_message_refused::InputMessageRefused;
pub use crate::events::input_message_unfit::InputMessageUnfit;
pub use crate::events::input_never_applied::{InputNeverApplied, Unapplied};
pub use crate::events::inputs_discarded::InputsDiscarded;
pub use crate::events::join_refused::JoinRefused;
pub use crate::events::journal_failed::JournalFailed;
pub use crate::events::journal_sync_slow::JournalSyncSlow;
pub use crate::events::link_lost::LinkLost;
pub use crate::events::listening::Listening;
pub use crate::events::match_started::MatchStarted;
pub use crate::events::order_dropped::OrderDropped;
pub use crate::events::orders_sent::OrdersSent;
pub use crate::events::receipt_refused::ReceiptRefused;
pub use crate::events::receipt_unsaved::ReceiptUnsaved;
pub use crate::events::save_refused::SaveRefused;
pub use crate::events::seeds_ran_out::SeedsRanOut;
pub use crate::events::session_aborted::SessionAborted;
pub use crate::events::session_refused::SessionRefused;
pub use crate::events::session_restored::SessionRestored;
pub use crate::events::session_written::SessionWritten;
pub use crate::events::ticks_caught_up::TicksCaughtUp;
pub use crate::events::time_dropped::TimeDropped;
pub use crate::sim_server::error::RestoreMatchError;

pub use crate::input_message::InputMessage;

pub use crate::local::local_pace::LocalPace;
pub use crate::local::local_relink::LocalRelink;
pub use crate::local::local_server::error::LocalServerError;
pub use crate::local::local_server::{LocalServer, LocalServerSetup, Relinks};
pub use crate::match_clock::MatchClock;
pub use crate::pace::{Pace, PaceSpeed};
pub use crate::save_command::SaveCommand;
pub use crate::sim_server::lobby::{Lobby, LobbySetup};

pub use crate::net_protocol::{InputChannel, NetProtocol};

pub use crate::order_script::{OrderScript, ScriptedInput, ScriptedOrder, ScriptedValue};
pub use crate::session_times::SessionTimes;
pub use crate::sim_client::bot_script::BotScript;
pub use crate::sim_client::client_dir::ClientDir;
pub use crate::sim_client::join_state::{JoinState, Loss};
pub use crate::sim_client::server_pin::ServerPin;
pub use crate::sim_client::{LeaveRequest, PendingOrders, PendingSaves, SimClient};
pub use crate::sim_server::SimServer;
pub use crate::sim_server::journal_watch::JournalWatch;
pub use crate::sim_server::key_file::KeyFile;
pub use crate::sim_server::key_file::error::KeyFileError;
pub use crate::sim_server::player_link::PlayerLink;
pub use crate::sim_server::server_bots::{ServerBots, SlotBot};
pub use crate::sim_server::server_dir::ServerDir;
pub use crate::sim_server::server_exit::ServerExit;
pub use crate::sim_server::server_setup::ServerSetup;
pub use crate::sim_server::session_dir::error::{
    AbortError, FindError, RestoreError, WaitingError,
};
pub use crate::sim_server::session_dir::snapshot_dir::SnapshotDir;
pub use crate::sim_server::session_dir::{RestoredSession, SessionDir, SessionFiles};
pub use crate::sim_server::session_journal::SessionJournal;
pub use crate::sim_server::tick_hashes::TickHashes;

#[cfg(feature = "bench")]
pub mod bench {
    pub use crate::sim_client::bench::{rollback, worst_client_frame};
}

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::harness::in_process_match::link_model::LinkModel;
    pub use crate::harness::in_process_match::{End, InProcessMatch, MatchSetup};
}
