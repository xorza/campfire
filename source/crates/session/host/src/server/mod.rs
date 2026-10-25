//! A session's server: its phase, its player slots and bots, its data directory with each session's
//! journal and snapshots, and its checkpoints.

pub(crate) mod bot_driver;
pub(crate) mod checkpoint_thread;
pub(crate) mod checkpoints;
pub(crate) mod error;
pub(crate) mod journal_watch;
pub(crate) mod player_link;
pub(crate) mod player_slot_bot_arg;
pub(crate) mod player_slot_links;
pub(crate) mod player_slots;
pub(crate) mod reports;
pub(crate) mod server_bots;
pub(crate) mod server_dir;
pub(crate) mod server_layout;
pub(crate) mod server_phase;
pub(crate) mod server_setup;
pub(crate) mod session_dir;
pub(crate) mod session_journal;
pub(crate) mod snapshot_dir;
#[cfg(feature = "internals")]
pub(crate) mod tick_hashes;
