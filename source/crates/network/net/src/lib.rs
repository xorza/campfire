//! Campfire's game networking on the stack: the protocol, the handshake, the server's and the
//! client's sessions, and their links.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod close_code;
mod error;
mod events;
mod faults;
mod hosting;
mod key_file;
mod local;
mod match_clock;
mod net_protocol;
mod os;
mod pace;
mod paced_runner;
mod process_exit;
#[cfg(feature = "internals")]
mod protocol_client;
mod session_times;
mod sim_client;
mod sim_server;
mod udp_links;
mod unix_clock;

pub use crate::close_code::CloseCode;
pub use crate::error::{JoinError, ReceiptRefusal, SaveRefusal};
pub use crate::events::bot_payload_dropped::DropCause;
pub use crate::events::checkpoint_failed::CheckpointFailed;
pub use crate::events::input_logged::InputLogged;
pub use crate::events::input_message_refused::InputMessageRefused;
pub use crate::events::inputs_discarded::InputsDiscarded;
pub use crate::events::join_refused::JoinRefused;
pub use crate::events::journal_cut::JournalCut;
pub use crate::events::journal_failed::JournalFailed;
pub use crate::events::link_lost::{LinkLossCause, LinkLost};
pub use crate::events::listening::Listening;
pub use crate::events::log_filled::LogFilled;
pub use crate::events::match_started::MatchStarted;
pub use crate::events::orders_sent::OrdersSent;
pub use crate::events::receipt_refused::ReceiptRefused;
pub use crate::events::receipt_unsaved::ReceiptUnsaved;
pub use crate::events::save_refused::SaveRefused;
pub use crate::events::seeds_ran_out::SeedsRanOut;
pub use crate::events::session_written::SessionWritten;
pub use crate::events::ticks_caught_up::TicksCaughtUp;
pub use crate::faults::error::SnapshotWriteError;
pub use crate::hosting::Hosting;
pub use crate::key_file::KeyFile;
pub use crate::key_file::error::KeyFileError;
pub use crate::local::local_link::LocalLink;
pub use crate::local::local_pace::LocalPacePlugin;
pub use crate::local::local_relink::LocalRelinkPlugin;
pub use crate::local::local_server::error::LocalServerError;
pub use crate::local::local_server::{LocalServer, LocalServerSetup, Relinks};
pub use crate::match_clock::MatchClock;
pub use crate::net_protocol::input_message::InputMessage;
pub use crate::net_protocol::join::JoinRole;
pub use crate::net_protocol::save_command::SaveCommand;
pub use crate::net_protocol::{InputChannel, NetProtocolPlugin};
pub use crate::os::Os;
pub use crate::pace::{Pace, PaceSpeed};
pub use crate::paced_runner::PacedRunnerPlugin;
pub use crate::process_exit::exit_code;
pub use crate::session_times::SessionTimes;
pub use crate::sim_client::client_dir::ClientDir;
pub use crate::sim_client::client_layout::ClientLayout;
pub use crate::sim_client::join_state::error::TermsMismatch;
pub use crate::sim_client::join_state::{JoinState, Loss};
pub use crate::sim_client::server_pin::ServerPin;
pub use crate::sim_client::session_signer::SessionSigner;
pub use crate::sim_client::{
    LeaveRequest, PendingOrders, PendingSaves, SimClientPlugin, WatchRequest,
};
pub use crate::sim_server::ServerFrameSystems;
pub use crate::sim_server::error::MatchBeginError;
pub use crate::sim_server::journal_watch::JournalWatch;
pub use crate::sim_server::lobby::error::{KeepFilesError, LobbyError, LobbyOpenError};
pub use crate::sim_server::lobby::{Lobby, LobbyOpening, LobbySetup, LobbyStart};
pub use crate::sim_server::pending_restore::{PendingRestore, UncheckedRestore};
pub use crate::sim_server::seat_bot_file::SeatBotFile;
pub use crate::sim_server::seat_bot_file::error::SeatBotFileError;
pub use crate::sim_server::server_bots::{SeatBot, SeatScript, ServerBots};
pub use crate::sim_server::server_dir::ServerDir;
pub use crate::sim_server::server_layout::ServerLayout;
pub use crate::sim_server::server_setup::ServerSetup;
pub use crate::sim_server::session_dir::error::{
    AbortError, FindError, RestoreError, SessionCreateError, SessionPublishError, WaitingError,
};
pub use crate::sim_server::session_dir::{Publisher, RestoredSession, SessionDir, SessionFiles};
pub use crate::sim_server::session_journal::SessionJournal;
pub use crate::sim_server::sim_server_plugins::SimServerPlugins;
pub use crate::sim_server::sim_server_world::{SessionOpening, SimServerWorld, StopRequest};
pub use crate::sim_server::snapshot_dir::SnapshotDir;
pub use crate::udp_links::UdpLinks;
pub use crate::unix_clock::UnixClock;

#[cfg(feature = "internals")]
pub mod internals {
    pub use crate::protocol_client::{ProtocolClient, ProtocolClientState};
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
    pub use crate::sim_server::tick_hashes::TickHashes;
}
