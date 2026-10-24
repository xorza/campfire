use std::cmp::Reverse;
use std::collections::BinaryHeap;

use campfire_common::{PlayerSlot, Tick};

use crate::input_rules::InputRules;
use crate::session_log::applied::Applied;
use crate::session_log::log_entries::{Entry, LogEntries, Served};
use crate::spill::Spill;

/// When each logged input applies, by the session's input rules: the inputs still to apply,
/// earliest first, those the tick last sealed applies, and where each player slot's inputs spill to
/// past a tick's max inputs. A replay drives it from the log's entries again, as they were
/// logged.
#[derive(Debug)]
pub(super) struct InputSchedule {
    rules: InputRules,
    /// Inputs applied in a tick not yet sealed, earliest first.
    pending: BinaryHeap<Reverse<InputOrder>>,
    /// Indices of the inputs applied in the tick last sealed.
    due: Vec<u32>,
    /// Each player slot's spill, by player slot. A spill counts its controller's inputs alone: a
    /// change of controller starts it afresh, so in a bot's player slot it counts the bot's.
    spills: Vec<Spill>,
}

/// Orders the inputs of one tick by player slot, then by when they were logged, which for one
/// player is their chain's order, whatever order the players' inputs arrived in, so the host cannot
/// choose who acts first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct InputOrder {
    pub(super) tick: Tick,
    pub(super) player_slot: PlayerSlot,
    pub(super) index: u32,
}

impl InputSchedule {
    /// The schedule of a session of `player_slots` player slots and of `rules`, which holds no
    /// input.
    pub(super) fn new(player_slots: usize, rules: InputRules) -> Self {
        Self {
            rules,
            pending: BinaryHeap::new(),
            due: Vec::new(),
            spills: vec![Spill::default(); player_slots],
        }
    }

    /// When an input stamped `stamp`, logged before `next`, applies: at its stamp, or at the next
    /// tick for one stamped before it however long before (05-protocol-spec.md, A late input
    /// applies at the next tick); never for one stamped past the max lead.
    pub(super) fn applied(&self, stamp: Tick, next: Tick) -> Applied {
        if stamp
            .since(next)
            .is_some_and(|lead| lead > self.rules.max_lead)
        {
            return Applied::Early;
        }
        Applied::At(stamp.max(next))
    }

    /// Queues the input `index` of `entries`, logged before `next`, for the tick it applies in, and
    /// says when that is: what `applied` says, or later, past the ticks its player slot's inputs
    /// before it fill. The log took it only as `admits` its place.
    pub(super) fn schedule(&mut self, entries: &LogEntries, index: u32, next: Tick) -> Applied {
        let input = entries.input(index);
        let outcome = self.applied(input.stamp, next);
        let Applied::At(earliest) = outcome else {
            return outcome;
        };
        let player_slot = input.player_slot;
        let spill = &mut self.spills[player_slot.index()];
        let place = spill
            .admit(earliest, next, &self.rules)
            .expect("a logged input's place is within the max lead");
        let tick = spill.take(place);
        self.pending.push(Reverse(InputOrder {
            tick,
            player_slot,
            index,
        }));
        Applied::At(tick)
    }

    /// Whether inputs of `player_slot` stamped `stamps`, in order, logged before `next`, each have
    /// a place within the max lead past `next`, as the log takes an input only then, so a player
    /// slot holds at most the max inputs per tick for each tick of the window.
    pub(super) fn admits(
        &self,
        player_slot: PlayerSlot,
        stamps: impl IntoIterator<Item = Tick>,
        next: Tick,
    ) -> bool {
        let mut spill = self.spills[player_slot.index()];
        stamps
            .into_iter()
            .all(|stamp| match self.applied(stamp, next) {
                Applied::At(earliest) => spill
                    .admit(earliest, next, &self.rules)
                    .map(|place| spill.take(place))
                    .is_some(),
                Applied::Early => true,
            })
    }

    /// The spill of `player_slot`.
    pub(super) fn spill(&self, player_slot: PlayerSlot) -> Spill {
        self.spills[player_slot.index()]
    }

    /// Drops the inputs of `player_slot` still to apply, and starts its spill afresh: its
    /// controller changes.
    pub(super) fn forget(&mut self, player_slot: PlayerSlot) {
        self.pending
            .retain(|Reverse(due)| due.player_slot != player_slot);
        self.spills[player_slot.index()] = Spill::default();
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

    /// The inputs the tick last sealed applies, by player slot, then in the order logged.
    pub(super) fn due(&self) -> &[u32] {
        &self.due
    }

    /// The schedule of `rules` at a boundary a checkpoint's carry records: each player slot's
    /// spill, by player slot, and the inputs still to apply there.
    pub(super) fn restored(
        rules: InputRules,
        spills: Vec<Spill>,
        pending: impl IntoIterator<Item = InputOrder>,
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

    /// The inputs still to apply, earliest first, then by player slot and in the order logged.
    pub(super) fn pending(&self) -> Vec<&InputOrder> {
        let mut due: Vec<&InputOrder> = self.pending.iter().map(|Reverse(due)| due).collect();
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
                Served::Join { player_slot, .. } | Served::Leave { player_slot, .. } => {
                    self.forget(player_slot);
                }
                Served::Renew { .. } | Served::Connected { .. } | Served::Disconnected { .. } => {}
            },
        }
    }
}
