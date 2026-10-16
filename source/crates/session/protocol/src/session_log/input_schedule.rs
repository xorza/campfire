use std::cmp::Reverse;
use std::collections::BinaryHeap;

use campfire_common::{PlayerSlot, Tick};

use crate::input_rules::InputRules;
use crate::session_log::applied::Applied;
use crate::session_log::log_entries::{Entry, LogEntries, Served};
use crate::spill::Spill;

/// When each logged input applies, by the session's input rules: the inputs still to apply,
/// earliest first, those the tick last sealed applies, and where each slot's inputs spill to
/// past a tick's max inputs. A replay drives it from the log's entries again, as they were
/// logged.
#[derive(Debug)]
pub(super) struct InputSchedule {
    rules: InputRules,
    /// Inputs applied in a tick not yet sealed, earliest first.
    pending: BinaryHeap<Reverse<Due>>,
    /// Indices of the inputs applied in the tick last sealed.
    due: Vec<u32>,
    /// Each slot's spill, by slot. A spill counts its controller's inputs alone: a change of
    /// controller starts it afresh, so in a bot's slot it counts the bot's.
    spills: Vec<Spill>,
}

/// Orders the inputs of one tick by slot, then by when they were logged, which for one player is
/// their chain's order, whatever order the players' inputs arrived in, so the host cannot choose
/// who acts first.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Due {
    pub(super) tick: Tick,
    pub(super) slot: PlayerSlot,
    pub(super) index: u32,
}

impl InputSchedule {
    /// The schedule of a session of `slots` slots and of `rules`, which holds no input.
    pub(super) fn new(slots: usize, rules: InputRules) -> Self {
        Self {
            rules,
            pending: BinaryHeap::new(),
            due: Vec::new(),
            spills: vec![Spill::default(); slots],
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
    /// and says when that is: what `applied` says, or later, past the ticks its slot's inputs
    /// before it fill.
    pub(super) fn schedule(&mut self, entries: &LogEntries, index: u32, next: Tick) -> Applied {
        let input = entries.input(index);
        let outcome = self.applied(input.stamp, next);
        let Applied::At(earliest) = outcome else {
            return outcome;
        };
        let slot = input.slot;
        let tick = self.spills[slot.index()].take(earliest, self.rules.max_per_tick);
        self.pending.push(Reverse(Due { tick, slot, index }));
        Applied::At(tick)
    }

    /// The tick a next input of `slot` that may apply from `next` on would apply in.
    pub(super) fn place(&self, slot: PlayerSlot, next: Tick) -> Tick {
        self.spills[slot.index()].place(next, self.rules.max_per_tick)
    }

    /// The spill of `slot`.
    pub(super) fn spill(&self, slot: PlayerSlot) -> Spill {
        self.spills[slot.index()]
    }

    /// Drops the inputs of `slot` still to apply, and starts its spill afresh: its controller
    /// changes.
    pub(super) fn forget(&mut self, slot: PlayerSlot) {
        self.pending.retain(|Reverse(due)| due.slot != slot);
        self.spills[slot.index()] = Spill::default();
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

    /// The inputs the tick last sealed applies, by slot, then in the order logged.
    pub(super) fn due(&self) -> &[u32] {
        &self.due
    }

    /// The inputs still to apply, earliest first, then by slot and in the order logged.
    pub(super) fn pending(&self) -> Vec<&Due> {
        let mut due: Vec<&Due> = self.pending.iter().map(|Reverse(due)| due).collect();
        due.sort_unstable();
        due
    }

    /// The schedule of no input, its spills afresh, as a replay starts it.
    pub(super) fn reset(&mut self) {
        self.pending.clear();
        self.due.clear();
        self.spills.fill(Spill::default());
    }

    /// Queues again what the logged entry `at` of `entries` scheduled before `next`, as a replay
    /// does; whether it is a server input.
    pub(super) fn replay(&mut self, entries: &LogEntries, at: u32, next: Tick) -> bool {
        match entries.entry(at) {
            Entry::Packet(packet) => {
                for index in entries.packet(packet).inputs.clone() {
                    self.schedule(entries, index, next);
                }
                false
            }
            Entry::Server(server) => {
                match entries.server(server).served {
                    Served::Bot { input } => {
                        self.schedule(entries, input, next);
                    }
                    Served::Join { slot, .. } | Served::Leave { slot, .. } => self.forget(slot),
                    Served::Renew { .. }
                    | Served::Connected { .. }
                    | Served::Disconnected { .. } => {}
                }
                true
            }
        }
    }
}
