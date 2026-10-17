use std::ops::Range;

use campfire_common::{AfterLeave, PlayerSlot, Tick};

use crate::delegation::Delegation;
use crate::player_input::PlayerInput;
use crate::server_input::{LeaveReason, ServerInput};
use crate::session_log::paged_bytes::{BytesAt, PagedBytes};
use crate::session_log::paged_vec::PagedVec;
use crate::session_log::{at, offset};
use crate::signature::Signature;

/// What a session's log holds as it was logged: every player's packet and server input in the
/// order they came, the inputs and payloads they carry, the delegations the players held, and
/// where each sealed tick's entries end. The whole session's grows by pages, so a record never
/// copies the history before it.
#[derive(Debug, Default)]
pub(super) struct LogEntries {
    inputs: PagedVec<LoggedInput>,
    payloads: PagedBytes,
    packets: PagedVec<Packet>,
    server: PagedVec<LoggedServer>,
    /// The packets and the server inputs, in the order they were logged.
    entries: PagedVec<Entry>,
    /// For each sealed tick, the end of the entries logged before it ran.
    tick_ends: PagedVec<u32>,
    /// Every delegation a player held, the header's and the joins' and renewals', which a
    /// player's controller and a join or a renewal name by index.
    delegations: Vec<Delegation>,
}

#[derive(Debug)]
struct LoggedInput {
    slot: PlayerSlot,
    stamp: Tick,
    payload: BytesAt,
}

/// A player's packet: its inputs, and the signature over the chain head after the last.
#[derive(Debug)]
pub(super) struct Packet {
    pub(super) inputs: Range<u32>,
    pub(super) signature: Signature,
}

/// A server input as logged, with its signature.
#[derive(Debug)]
pub(super) struct LoggedServer {
    pub(super) served: Served,
    pub(super) signature: Signature,
}

/// A logged server input, as the log holds it: a bot's commands by the input they are, and a
/// delegation by its index among the log's, so the log keeps each payload and delegation once.
#[derive(Debug, Clone, Copy)]
pub(super) enum Served {
    Bot {
        input: u32,
    },
    Join {
        slot: PlayerSlot,
        delegation: u32,
    },
    Renew {
        slot: PlayerSlot,
        delegation: u32,
    },
    Leave {
        slot: PlayerSlot,
        reason: LeaveReason,
        becomes: AfterLeave,
    },
    Connected {
        slot: PlayerSlot,
    },
    Disconnected {
        slot: PlayerSlot,
    },
}

/// A logged entry, by its place among the packets or the server inputs.
#[derive(Debug, Clone, Copy)]
pub(super) enum Entry {
    Packet(u32),
    Server(u32),
}

impl LogEntries {
    /// The tick that the entries logged now arrive before.
    pub(super) fn next_tick(&self) -> Tick {
        Tick::new(u64::try_from(self.tick_ends.len()).expect("tick count fits u64"))
    }

    /// How many entries the log holds.
    pub(super) fn len(&self) -> u32 {
        offset(self.entries.len())
    }

    /// Where the entries of the tick last sealed end, and those since start.
    pub(super) fn sealed_end(&self) -> u32 {
        self.tick_ends.last().copied().unwrap_or(0)
    }

    /// Where the entries of `tick`, a sealed tick, end.
    pub(super) fn tick_end(&self, tick: Tick) -> u32 {
        self.tick_ends[usize::try_from(tick.get()).expect("tick fits usize")]
    }

    /// How many entries were logged since the last tick was sealed.
    pub(super) fn since_sealed(&self) -> u32 {
        self.len() - self.sealed_end()
    }

    /// Whether no entry was logged since the last tick was sealed: where a checkpoint may come.
    pub(super) fn at_boundary(&self) -> bool {
        self.since_sealed() == 0
    }

    /// Where each sealed tick's entries end, by tick.
    pub(super) fn tick_ends(&self) -> impl Iterator<Item = u32> + '_ {
        self.tick_ends.iter().copied()
    }

    /// Seals the next tick, its entries ending at `end`.
    pub(super) fn seal(&mut self, end: u32) {
        debug_assert!(end <= self.len(), "a tick ends within the entries");
        self.tick_ends.push(end);
    }

    pub(super) fn entry(&self, index: u32) -> Entry {
        self.entries[at(index)]
    }

    /// The entries at `range`, in the order logged.
    pub(super) fn entries(&self, range: Range<u32>) -> impl Iterator<Item = Entry> + '_ {
        self.entries.range(at(range.start)..at(range.end)).copied()
    }

    pub(super) fn packet(&self, index: u32) -> &Packet {
        &self.packets[at(index)]
    }

    pub(super) fn server(&self, index: u32) -> &LoggedServer {
        &self.server[at(index)]
    }

    /// How many inputs the log holds.
    pub(super) fn inputs_len(&self) -> u32 {
        offset(self.inputs.len())
    }

    /// The input `index` logged.
    pub(super) fn input(&self, index: u32) -> PlayerInput<'_> {
        let logged = &self.inputs[at(index)];
        PlayerInput {
            slot: logged.slot,
            stamp: logged.stamp,
            payload: self.payloads.get(logged.payload),
        }
    }

    /// The delegation of index `index`.
    pub(super) fn delegation(&self, index: u32) -> &Delegation {
        &self.delegations[at(index)]
    }

    /// Logs `input`'s payload, and gives its index.
    pub(super) fn push_input(&mut self, input: PlayerInput<'_>) -> u32 {
        let index = self.inputs_len();
        let payload = self.payloads.push(input.payload);
        self.inputs.push(LoggedInput {
            slot: input.slot,
            stamp: input.stamp,
            payload,
        });
        index
    }

    /// Keeps `delegation`, the header's, a join's or a renewal's, and gives its index.
    pub(super) fn push_delegation(&mut self, delegation: Delegation) -> u32 {
        let index = offset(self.delegations.len());
        self.delegations.push(delegation);
        index
    }

    /// Logs the packet of the inputs at `inputs`, signed by `signature`, as the next entry.
    pub(super) fn push_packet(&mut self, inputs: Range<u32>, signature: Signature) {
        self.entries.push(Entry::Packet(offset(self.packets.len())));
        self.packets.push(Packet { inputs, signature });
    }

    /// Logs `served`, signed by `signature`, as the next entry.
    pub(super) fn push_server(&mut self, served: Served, signature: Signature) {
        self.entries.push(Entry::Server(offset(self.server.len())));
        self.server.push(LoggedServer { served, signature });
    }

    /// The server input `served` holds.
    pub(super) fn server_input(&self, served: Served) -> ServerInput<'_> {
        match served {
            Served::Bot { input } => {
                let input = self.input(input);
                ServerInput::Bot {
                    slot: input.slot,
                    payload: input.payload,
                }
            }
            Served::Join { slot, delegation } => ServerInput::Join {
                slot,
                delegation: self.delegation(delegation).clone(),
            },
            Served::Renew { slot, delegation } => ServerInput::Renew {
                slot,
                delegation: self.delegation(delegation).clone(),
            },
            Served::Leave {
                slot,
                reason,
                becomes,
            } => ServerInput::Leave {
                slot,
                reason,
                becomes,
            },
            Served::Connected { slot } => ServerInput::Connected { slot },
            Served::Disconnected { slot } => ServerInput::Disconnected { slot },
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::session_log::log_entries::{LogEntries, LoggedServer};

    impl LogEntries {
        /// Every server input logged so far, in the order logged.
        pub(crate) fn servers(&self) -> impl Iterator<Item = &LoggedServer> {
            self.server.iter()
        }
    }
}
