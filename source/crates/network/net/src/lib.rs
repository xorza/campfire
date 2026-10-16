//! Lightyear transport, handshake and replication.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod bot_script;
mod close_code;
mod error;
mod events;
mod faults;
mod hosting;
mod key_file;
mod local;
mod match_clock;
mod net_protocol;
mod order_script;
mod os;
mod pace;
mod process_exit;
mod session_times;
mod sim_client;
mod sim_server;

pub use crate::bot_script::BotScript;
pub use crate::bot_script::error::UnitsMissing;
pub use crate::close_code::CloseCode;
pub use crate::error::{JoinError, ReceiptRefusal, SaveRefusal};
pub use crate::events::bot_payload_dropped::{BotPayloadDropped, DropCause};
pub use crate::events::bot_units_missing::BotUnitsMissing;
pub use crate::events::checkpoint_failed::CheckpointFailed;
pub use crate::events::checkpoint_taken::CheckpointTaken;
pub use crate::events::client_frame::ClientFrame;
pub use crate::events::input_dropped::InputDropped;
pub use crate::events::input_logged::InputLogged;
pub use crate::events::input_message_refused::InputMessageRefused;
pub use crate::events::input_message_unfit::InputMessageUnfit;
pub use crate::events::input_never_applied::{InputNeverApplied, Unapplied};
pub use crate::events::inputs_discarded::InputsDiscarded;
pub use crate::events::join_refused::JoinRefused;
pub use crate::events::journal_failed::JournalFailed;
pub use crate::events::journal_sync_slow::JournalSyncSlow;
pub use crate::events::link_lost::{LinkLossCause, LinkLost};
pub use crate::events::listening::Listening;
pub use crate::events::log_filled::LogFilled;
pub use crate::events::match_started::MatchStarted;
pub use crate::events::order_dropped::OrderDropped;
pub use crate::events::orders_sent::OrdersSent;
pub use crate::events::receipt_refused::ReceiptRefused;
pub use crate::events::receipt_unsaved::ReceiptUnsaved;
pub use crate::events::save_refused::SaveRefused;
pub use crate::events::seeds_ran_out::SeedsRanOut;
pub use crate::events::server_frame::ServerFrame;
pub use crate::events::session_aborted::SessionAborted;
pub use crate::events::session_refused::SessionRefused;
pub use crate::events::session_restored::SessionRestored;
pub use crate::events::session_written::SessionWritten;
pub use crate::events::ticks_caught_up::TicksCaughtUp;
pub use crate::events::time_dropped::TimeDropped;
pub use crate::faults::error::SnapshotWriteError;
pub use crate::hosting::Hosting;
pub use crate::key_file::KeyFile;
pub use crate::key_file::error::KeyFileError;
pub use crate::local::local_link::LocalLink;
pub use crate::local::local_pace::LocalPace;
pub use crate::local::local_relink::LocalRelink;
pub use crate::local::local_server::error::LocalServerError;
pub use crate::local::local_server::{LocalServer, LocalServerSetup, Relinks};
pub use crate::match_clock::MatchClock;
pub use crate::net_protocol::input_message::InputMessage;
pub use crate::net_protocol::join::JoinRole;
pub use crate::net_protocol::save_command::SaveCommand;
pub use crate::net_protocol::{InputChannel, NetProtocol};
pub use crate::order_script::error::{OrderScriptError, OrderScriptReadError, UnknownUnitType};
pub use crate::order_script::{
    OrderScript, ScriptedInput, ScriptedOrder, ScriptedUnits, ScriptedValue,
};
pub use crate::os::{Clock, Os};
pub use crate::pace::{Pace, PaceSpeed};
pub use crate::process_exit::exit_code;
pub use crate::session_times::SessionTimes;
pub use crate::sim_client::client_dir::ClientDir;
pub use crate::sim_client::client_layout::ClientLayout;
pub use crate::sim_client::join_state::error::TermsMismatch;
pub use crate::sim_client::join_state::{JoinState, Loss};
pub use crate::sim_client::server_pin::ServerPin;
pub use crate::sim_client::{LeaveRequest, PendingOrders, PendingSaves, SimClient, WatchRequest};
pub use crate::sim_server::error::RestoreMatchError;
pub use crate::sim_server::journal_watch::JournalWatch;
pub use crate::sim_server::lobby::error::{KeepFilesError, LobbyError, LobbyOpenError};
pub use crate::sim_server::lobby::{Lobby, LobbyOpening, LobbySetup};
pub use crate::sim_server::pending_restore::PendingRestore;
pub use crate::sim_server::server_bots::{ServerBots, SlotBot};
pub use crate::sim_server::server_dir::ServerDir;
pub use crate::sim_server::server_layout::ServerLayout;
pub use crate::sim_server::server_setup::ServerSetup;
pub use crate::sim_server::session_dir::error::{
    AbortError, FindError, PublishError, RestoreError, SessionCreateError, WaitingError,
};
pub use crate::sim_server::session_dir::{Publisher, RestoredSession, SessionDir, SessionFiles};
pub use crate::sim_server::session_journal::SessionJournal;
pub use crate::sim_server::sim_server_plugins::SimServerPlugins;
pub use crate::sim_server::sim_server_world::{SimServerWorld, StopRequest};
pub use crate::sim_server::slot_bot_file::SlotBotFile;
pub use crate::sim_server::slot_bot_file::error::SlotBotFileError;
pub use crate::sim_server::snapshot_dir::SnapshotDir;
pub use crate::sim_server::tick_hashes::TickHashes;

#[cfg(feature = "internals")]
pub mod internals {
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use crate::sim_client::sim_client_world::internals::SimClientWorldInternals;
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use crate::sim_server::sim_server_world::internals::SimServerWorldInternals;
}
