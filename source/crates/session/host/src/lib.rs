//! A session's host, apart from the network stack: the server's phase, player slots, bots, data and
//! checkpoints, the client's join state and the inputs it sent, the session's messages, the
//! clocks both ends read, and the events their logs hold.

#![expect(
    clippy::needless_pass_by_value,
    reason = "Bevy systems take `Res` and `Query` by value"
)]

mod client;
mod error;
mod events;
mod faults;
mod hosting;
mod key_file;
mod match_clock;
mod messages;
mod order_file;
mod os;
mod pace;
mod process_exit;
mod server;
mod session_times;
mod unix_clock;

pub use crate::client::client_dir::ClientDir;
pub use crate::client::client_layout::ClientLayout;
pub use crate::client::join_state::error::TermsMismatch;
pub use crate::client::join_state::{
    JoinState, JoinedSession, Joining, LinkLoss, Loss, Member, Playing, Retry, StartEffect,
};
pub use crate::client::known_commanded::KnownCommanded;
pub use crate::client::player_signer::PlayerSigner;
pub use crate::client::receipt_writer::ReceiptWriter;
pub use crate::client::sent_inputs::SentInputs;
pub use crate::client::server_pin::ServerPin;
pub use crate::client::session_signer::SessionSigner;
pub use crate::error::{JoinError, OrderFileError, ReceiptRefusal, SaveRefusal};
pub use crate::events::bot_order_unsent::BotOrderUnsent;
pub use crate::events::checkpoint_failed::CheckpointFailed;
pub use crate::events::client_frame::ClientFrame;
pub use crate::events::input_dropped::InputDropped;
pub use crate::events::input_logged::InputLogged;
pub use crate::events::input_message_refused::InputMessageRefused;
pub use crate::events::input_message_unfit::InputMessageUnfit;
pub use crate::events::input_never_applied::{InputNeverApplied, Unapplied};
pub use crate::events::inputs_discarded::InputsDiscarded;
pub use crate::events::join_refused::JoinRefused;
pub use crate::events::journal_cut::JournalCut;
pub use crate::events::journal_failed::JournalFailed;
pub use crate::events::listening::Listening;
pub use crate::events::load_failed::LoadFailed;
pub use crate::events::log_filled::LogFilled;
pub use crate::events::match_ended::MatchEnded;
pub use crate::events::match_started::MatchStarted;
pub use crate::events::order_dropped::OrderDropped;
pub use crate::events::orders_sent::OrdersSent;
pub use crate::events::receipt_refused::ReceiptRefused;
pub use crate::events::receipt_unsaved::ReceiptUnsaved;
pub use crate::events::save_loaded::SaveLoaded;
pub use crate::events::save_refused::SaveRefused;
pub use crate::events::script_printed::ScriptPrinted;
pub use crate::events::seeds_ran_out::SeedsRanOut;
pub use crate::events::server_frame::ServerFrame;
pub use crate::events::session_refused::SessionRefused;
pub use crate::events::session_restored::SessionRestored;
pub use crate::events::session_written::{SessionEnd, SessionWritten, UnknownEnd};
pub use crate::events::ticks_caught_up::TicksCaughtUp;
pub use crate::events::time_dropped::TimeDropped;
pub use crate::events::unit_died::UnitDied;
pub use crate::faults::WorkerFaults;
pub use crate::faults::error::{ReceiptWriteError, SnapshotReadError, SnapshotWriteError};
pub use crate::faults::fault::Fault;
pub use crate::hosting::Hosting;
pub use crate::key_file::KeyFile;
pub use crate::key_file::error::KeyFileError;
pub use crate::match_clock::MatchClock;
pub use crate::messages::dismissed::Dismissed;
pub use crate::messages::input_ack::InputAck;
pub use crate::messages::input_message::InputMessage;
pub use crate::messages::join::{Join, JoinRole};
pub use crate::messages::joined::Joined;
pub use crate::messages::leave_match::LeaveMatch;
pub use crate::messages::match_loaded::MatchLoaded;
pub use crate::messages::match_start::{ChainHead, MatchStart, PlayerSlotChain};
pub use crate::messages::offer::SessionOffer;
pub use crate::messages::save_command::SaveCommand;
pub use crate::messages::watch_start::WatchStart;
pub use crate::messages::watch_teams::WatchTeams;
pub use crate::order_file::OrderFile;
pub use crate::os::Os;
pub use crate::pace::{Pace, PaceSpeed};
pub use crate::process_exit::exit_code;
pub use crate::server::bot_driver::BotDriver;
pub use crate::server::checkpoints::{Checkpoints, SaveTaken};
pub use crate::server::error::MatchBeginError;
pub use crate::server::journal_watch::{JournalFrom, JournalWatch};
pub use crate::server::player_link::PlayerLink;
pub use crate::server::player_slot_bot_arg::error::PlayerSlotBotArgError;
pub use crate::server::player_slot_bot_arg::{BotArg, PlayerSlotBotArg};
pub use crate::server::player_slot_links::{PlayerSlotLink, PlayerSlotLinks};
pub use crate::server::player_slots::PlayerSlots;
pub use crate::server::server_bots::error::ServerBotsError;
pub use crate::server::server_bots::{BotPlan, BotPlay, PlannedBot, PlayerSlotBot, ServerBots};
pub use crate::server::server_dir::ServerDir;
pub use crate::server::server_layout::ServerLayout;
pub use crate::server::server_phase::ServerPhase;
pub use crate::server::server_setup::ServerSetup;
pub use crate::server::session_dir::error::{
    AbortError, FindError, RestoreError, SessionCreateError, SessionPublishError, WaitingError,
};
pub use crate::server::session_dir::{
    Publisher, ReplayWith, Replayed, RestoredSession, SessionDir, SessionFiles,
};
pub use crate::server::session_journal::SessionJournal;
pub use crate::server::snapshot_dir::SnapshotDir;
pub use crate::session_times::SessionTimes;
pub use crate::unix_clock::UnixClock;

#[cfg(feature = "internals")]
pub mod internals {
    #[expect(
        clippy::module_name_repetitions,
        reason = "a module's world trait for tests and benches is `<Module>WorldInternals` (AGENTS.md)"
    )]
    pub use crate::client::sim_client_world::internals::SimClientWorldInternals;
    pub use crate::server::tick_hashes::TickHashes;
}
