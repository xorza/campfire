use std::ops::Range;

use campfire_common::{AfterLeave, Seat, Tick};

use crate::delegation::Delegation;
use crate::player_input::PlayerInput;
use crate::server_input::{LeaveReason, ServerInput};
use crate::session_log::paged_bytes::{BytesAt, BytesEnd, PagedBytes};
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
    packets: PagedVec<SignedInputs>,
    server: PagedVec<LoggedServer>,
    /// The packets and the server inputs, in the order they were logged.
    entries: PagedVec<Entry>,
    /// For each sealed tick, the end of the entries logged before it ran.
    tick_ends: PagedVec<u32>,
    /// Every delegation a player held, the header's and the joins' and renewals', which a
    /// player's controller and a join or a renewal name by index.
    delegations: Vec<Delegation>,
}

/// Where each of the log's buffers ends at a boundary: what a load cuts the log back to, and what
/// a server's log forgets the past before, as its journal holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct EntriesAt {
    pub(super) entries: u32,
    pub(super) inputs: u32,
    payloads: BytesEnd,
    packets: u32,
    server: u32,
    delegations: u32,
    ticks: u32,
}

#[derive(Debug)]
struct LoggedInput {
    seat: Seat,
    stamp: Tick,
    payload: BytesAt,
}

/// A player's packet: its inputs, and the signature over the chain head after the last.
#[derive(Debug)]
pub(super) struct SignedInputs {
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
        seat: Seat,
        delegation: u32,
    },
    Renew {
        seat: Seat,
        delegation: u32,
    },
    Leave {
        seat: Seat,
        reason: LeaveReason,
        becomes: AfterLeave,
    },
    Connected {
        seat: Seat,
    },
    Disconnected {
        seat: Seat,
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
    pub(super) fn is_at_boundary(&self) -> bool {
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

    /// Starts the entries at the boundary before `tick`, a checkpoint's, as a log file that
    /// starts there holds none before it: the ticks before `tick` hold no end, but the one before
    /// it, where the entries since start.
    pub(super) fn start_at(&mut self, tick: Tick) {
        debug_assert!(
            self.entries.len() == 0 && self.tick_ends.len() == 0,
            "entries start at a boundary before any is logged"
        );
        let before = tick
            .get()
            .checked_sub(1)
            .expect("a checkpoint's boundary after tick 0");
        self.tick_ends = PagedVec::starting_at(usize::try_from(before).expect("tick fits usize"));
        self.tick_ends.push(0);
    }

    /// Where its buffers end now.
    pub(super) fn at(&self) -> EntriesAt {
        EntriesAt {
            entries: self.len(),
            inputs: self.inputs_len(),
            payloads: self.payloads.end(),
            packets: offset(self.packets.len()),
            server: offset(self.server.len()),
            delegations: offset(self.delegations.len()),
            ticks: offset(self.tick_ends.len()),
        }
    }

    /// Drops what was logged after `end`, as a load goes back to its save.
    pub(super) fn truncate(&mut self, end: EntriesAt) {
        self.entries.truncate(at(end.entries));
        self.inputs.truncate(at(end.inputs));
        self.payloads.truncate(end.payloads);
        self.packets.truncate(at(end.packets));
        self.server.truncate(at(end.server));
        self.delegations.truncate(at(end.delegations));
        self.tick_ends.truncate(at(end.ticks));
    }

    /// Forgets the entries and the tick ends before `end`, and the inputs before `inputs`, at most
    /// `end`'s, with their payloads: the past a server's journal holds. Each item it keeps keeps
    /// its index, and the delegations stay.
    pub(super) fn forget_before(&mut self, end: EntriesAt, inputs: u32) {
        debug_assert!(
            inputs <= end.inputs,
            "the inputs kept start by the boundary"
        );
        self.entries.forget_before(at(end.entries));
        self.packets.forget_before(at(end.packets));
        self.server.forget_before(at(end.server));
        // The tick before the boundary's end stays, as the entries since it count from it.
        self.tick_ends
            .forget_before(at(end.ticks.saturating_sub(1)));
        let first = self.inputs.get(at(inputs)).map(|input| input.payload);
        self.inputs.forget_before(at(inputs));
        self.payloads.forget_before(first);
    }

    /// The index of `delegation` among the log's, the last that equals it.
    pub(super) fn delegation_index(&self, delegation: &Delegation) -> Option<u32> {
        let held = self
            .delegations
            .iter()
            .rposition(|held| held == delegation)?;
        Some(offset(held))
    }

    pub(super) fn entry(&self, index: u32) -> Entry {
        self.entries[at(index)]
    }

    /// The entries at `range`, in the order logged.
    pub(super) fn entries(&self, range: Range<u32>) -> impl Iterator<Item = Entry> + '_ {
        self.entries.range(at(range.start)..at(range.end)).copied()
    }

    pub(super) fn packet(&self, index: u32) -> &SignedInputs {
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
            seat: logged.seat,
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
            seat: input.seat,
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
        self.packets.push(SignedInputs { inputs, signature });
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
                    seat: input.seat,
                    payload: input.payload,
                }
            }
            Served::Join { seat, delegation } => ServerInput::Join {
                seat,
                delegation: self.delegation(delegation).clone(),
            },
            Served::Renew { seat, delegation } => ServerInput::Renew {
                seat,
                delegation: self.delegation(delegation).clone(),
            },
            Served::Leave {
                seat,
                reason,
                becomes,
            } => ServerInput::Leave {
                seat,
                reason,
                becomes,
            },
            Served::Connected { seat } => ServerInput::Connected { seat },
            Served::Disconnected { seat } => ServerInput::Disconnected { seat },
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
