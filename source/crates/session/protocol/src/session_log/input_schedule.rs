use std::cmp::Reverse;
use std::collections::BinaryHeap;

use campfire_common::{Seat, Tick};

use crate::input_rules::InputRules;
use crate::session_log::applied::Applied;
use crate::session_log::log_entries::{Entry, LogEntries, Served};
use crate::spill::Spill;

/// When each logged input applies, by the session's input rules: the inputs still to apply,
/// earliest first, those the tick last sealed applies, and where each seat's inputs spill to
/// past a tick's max inputs. A replay drives it from the log's entries again, as they were
/// logged.
#[derive(Debug)]
pub(super) struct InputSchedule {
    rules: InputRules,
    /// Inputs applied in a tick not yet sealed, earliest first.
    pending: BinaryHeap<Reverse<Due>>,
    /// Indices of the inputs applied in the tick last sealed.
    due: Vec<u32>,
    /// Each seat's spill, by seat. A spill counts its controller's inputs alone: a change of
    /// controller starts it afresh, so in a bot's seat it counts the bot's.
    spills: Vec<Spill>,
}

/// Orders the inputs of one tick by seat, then by when they were logged, which for one player is
/// their chain's order, whatever order the players' inputs arrived in, so the host cannot choose
/// who acts first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Due {
    pub(super) tick: Tick,
    pub(super) seat: Seat,
    pub(super) index: u32,
}

impl InputSchedule {
    /// The schedule of a session of `seats` seats and of `rules`, which holds no input.
    pub(super) fn new(seats: usize, rules: InputRules) -> Self {
        Self {
            rules,
            pending: BinaryHeap::new(),
            due: Vec::new(),
            spills: vec![Spill::default(); seats],
        }
    }

    /// When an input stamped `stamp`, logged before `next`, applies.
    pub(super) fn applied(&self, stamp: Tick, next: Tick) -> Applied {
        if next
            .since(stamp)
            .is_some_and(|delay| delay > self.rules.max_delay)
        {
            return Applied::Late;
        }
        if stamp
            .since(next)
            .is_some_and(|lead| lead > self.rules.max_lead)
        {
            return Applied::Early;
        }
        Applied::At(stamp.max(next))
    }

    /// Queues the input `index` of `entries`, logged before `next`, for the tick it applies in,
    /// and says when that is: what `applied` says, or later, past the ticks its seat's inputs
    /// before it fill.
    pub(super) fn schedule(&mut self, entries: &LogEntries, index: u32, next: Tick) -> Applied {
        let input = entries.input(index);
        let outcome = self.applied(input.stamp, next);
        let Applied::At(earliest) = outcome else {
            return outcome;
        };
        let seat = input.seat;
        let tick = self.spills[seat.index()].take(earliest, self.rules.max_per_tick);
        self.pending.push(Reverse(Due { tick, seat, index }));
        Applied::At(tick)
    }

    /// The tick a next input of `seat` that may apply from `next` on would apply in.
    pub(super) fn place(&self, seat: Seat, next: Tick) -> Tick {
        self.spills[seat.index()].place(next, self.rules.max_per_tick)
    }

    /// The spill of `seat`.
    pub(super) fn spill(&self, seat: Seat) -> Spill {
        self.spills[seat.index()]
    }

    /// Drops the inputs of `seat` still to apply, and starts its spill afresh: its controller
    /// changes.
    pub(super) fn forget(&mut self, seat: Seat) {
        self.pending.retain(|Reverse(due)| due.seat != seat);
        self.spills[seat.index()] = Spill::default();
    }

    /// Takes the inputs that apply in `tick`, which the schedule then gives as `due`.
    pub(super) fn seal(&mut self, tick: Tick) {
        self.due.clear();
        while let Some(Reverse(due)) = self.pending.peek()
            && due.tick == tick
        {
            self.due.push(due.index);
            self.pending.pop();
        }
    }

    /// The inputs the tick last sealed applies, by seat, then in the order logged.
    pub(super) fn due(&self) -> &[u32] {
        &self.due
    }

    /// The schedule of `rules` at a boundary a checkpoint's carry records: each seat's spill, by
    /// seat, and the inputs still to apply there.
    pub(super) fn restored(
        rules: InputRules,
        spills: Vec<Spill>,
        pending: impl IntoIterator<Item = Due>,
    ) -> Self {
        Self {
            rules,
            pending: pending.into_iter().map(Reverse).collect(),
            due: Vec::new(),
            spills,
        }
    }

    /// The first logged of the inputs still to apply; none when none is.
    pub(super) fn earliest_index(&self) -> Option<u32> {
        self.pending.iter().map(|Reverse(due)| due.index).min()
    }

    /// The inputs still to apply, earliest first, then by seat and in the order logged.
    pub(super) fn pending(&self) -> Vec<&Due> {
        let mut due: Vec<&Due> = self.pending.iter().map(|Reverse(due)| due).collect();
        due.sort_unstable();
        due
    }

    /// Queues again what the logged entry `at` of `entries` scheduled before `next`, as a replay
    /// does.
    pub(super) fn replay(&mut self, entries: &LogEntries, at: u32, next: Tick) {
        match entries.entry(at) {
            Entry::Packet(packet) => {
                for index in entries.packet(packet).inputs.clone() {
                    self.schedule(entries, index, next);
                }
            }
            Entry::Server(server) => match entries.server(server).served {
                Served::Bot { input } => {
                    self.schedule(entries, input, next);
                }
                Served::Join { seat, .. } | Served::Leave { seat, .. } => self.forget(seat),
                Served::Renew { .. } | Served::Connected { .. } | Served::Disconnected { .. } => {}
            },
        }
    }
}
