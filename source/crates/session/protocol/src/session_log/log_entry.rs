use std::ops::Range;

use campfire_common::Tick;

use crate::player_input::PlayerInput;
use crate::server_input::ServerInput;
use crate::session_log::log_entries::{Entry, LogEntries, Served};

/// An entry a log holds, as `SessionLog::tick_entries` gives them in the order logged: a player's
/// packet, or a server input.
#[derive(Debug, Clone)]
pub enum LogEntry<'l> {
    Packet(LoggedPacket<'l>),
    Server(LoggedServerInput<'l>),
}

/// Where the entries a log holds start: the segment, and the tick it starts at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldStart {
    pub segment: u32,
    pub tick: Tick,
}

/// A server input as the log holds it.
#[derive(Debug, Clone, Copy)]
pub struct LoggedServerInput<'l> {
    entries: &'l LogEntries,
    served: Served,
}

impl<'l> LoggedServerInput<'l> {
    /// The input, as the server signed it.
    pub fn input(self) -> ServerInput<'l> {
        self.entries.server_input(self.served)
    }
}

/// A player's packet as the log holds it: its inputs, in the order the packet held them.
#[derive(Debug, Clone)]
pub struct LoggedPacket<'l> {
    entries: &'l LogEntries,
    inputs: Range<u32>,
}

impl<'l> LoggedPacket<'l> {
    /// Its inputs, each of its player slot, in order.
    pub fn inputs(&self) -> impl ExactSizeIterator<Item = PlayerInput<'l>> + use<'l> {
        let entries = self.entries;
        self.inputs.clone().map(move |index| entries.input(index))
    }
}

impl<'l> LogEntry<'l> {
    /// The entry `entry` of `entries`.
    pub(super) fn of(entries: &'l LogEntries, entry: Entry) -> Self {
        match entry {
            Entry::Packet(index) => Self::Packet(LoggedPacket {
                entries,
                inputs: entries.packet(index).inputs.clone(),
            }),
            Entry::Server(index) => Self::Server(LoggedServerInput {
                entries,
                served: entries.server(index).served,
            }),
        }
    }
}
