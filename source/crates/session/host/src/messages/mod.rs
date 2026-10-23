//! The messages of a session between a client and its server, which `net` sends on the stack.

pub(crate) mod dismissed;
pub(crate) mod input_ack;
pub(crate) mod input_message;
pub(crate) mod join;
pub(crate) mod joined;
pub(crate) mod leave_match;
pub(crate) mod match_loaded;
pub(crate) mod match_start;
pub(crate) mod offer;
pub(crate) mod save_command;
pub(crate) mod watch_start;
pub(crate) mod watch_teams;
