use std::collections::VecDeque;
use std::ops::Range;

use bevy_ecs::resource::Resource;
use campfire_common::Tick;
use campfire_sim::StableId;

/// What the client knows of the units the server commands for its player: those of the last of
/// its selects the server applied, while none it sent waits for the server's word, as an input
/// the server logs early never takes effect, so an order sent behind a select it never applied
/// would command the units before it (04-capabilities/control.md, A select goes with each order
/// until its sender knows the server commands those units).
#[derive(Resource, Debug, Default)]
pub struct KnownCommanded {
    /// The units of the last whole select the server applied, when that is what it commands.
    applied: Vec<StableId>,
    known: bool,
    waiting: SelectQueue,
}

/// The selects sent and not acknowledged yet, in the order of their seqs, with their units end to
/// end in one buffer that holds theirs alone, so a client that always has one in flight holds no
/// more than those.
#[derive(Debug, Default)]
struct SelectQueue {
    selects: VecDeque<WaitingSelect>,
    units: Vec<StableId>,
}

/// A select the client sent: its input's seq, and its units, or none for a part of a select past
/// one payload, which leaves the server commanding what the client cannot tell alone.
#[derive(Debug)]
struct WaitingSelect {
    seq: u64,
    units: Option<Range<usize>>,
}

impl KnownCommanded {
    /// Whether the server commands exactly `units` for the player, so an order to them needs no
    /// select.
    pub fn commands(&self, units: &[StableId]) -> bool {
        self.known && self.waiting.selects.is_empty() && self.applied == units
    }

    /// Keeps that the input of `seq` holds a whole select of `units`.
    pub fn sent(&mut self, seq: u64, units: &[StableId]) {
        self.waiting.push(seq, Some(units));
    }

    /// Keeps that the input of `seq` holds a part of a select past one payload.
    pub fn sent_part(&mut self, seq: u64) {
        self.waiting.push(seq, None);
    }

    /// Takes the server's word on the inputs from seq `first` on, each applied in the tick
    /// `applied` gives or never: a whole select applied leaves its units commanded, a part applied
    /// leaves the client not knowing, and one never applied changes nothing. The server answers in
    /// the order of the seqs, so a select before them that waits never applied.
    pub fn acknowledge(&mut self, first: u64, applied: &[Option<Tick>]) {
        let end = first + u64::try_from(applied.len()).expect("a count fits u64");
        let Self {
            applied: held,
            known,
            waiting,
        } = self;
        waiting.pop_before(end, |seq, units| {
            let at = seq
                .checked_sub(first)
                .and_then(|at| usize::try_from(at).ok());
            if at.and_then(|at| applied[at]).is_none() {
                return;
            }
            match units {
                Some(units) => {
                    held.clear();
                    held.extend_from_slice(units);
                    *known = true;
                }
                None => *known = false,
            }
        });
    }

    /// Forgets all it knew, as a client whose link failed does: the server's word on its inputs
    /// no longer comes.
    pub fn clear(&mut self) {
        self.applied.clear();
        self.known = false;
        self.waiting.selects.clear();
        self.waiting.units.clear();
    }
}

impl SelectQueue {
    /// Keeps the select of `seq`, of `units` or of a part.
    fn push(&mut self, seq: u64, units: Option<&[StableId]>) {
        debug_assert!(
            self.selects.back().is_none_or(|last| last.seq < seq),
            "selects are sent in the order of their seqs"
        );
        let units = units.map(|units| {
            let start = self.units.len();
            self.units.extend_from_slice(units);
            start..self.units.len()
        });
        self.selects.push_back(WaitingSelect { seq, units });
    }

    /// Takes out each select of a seq before `end`, first to last, with its units, and drops
    /// their units.
    fn pop_before(&mut self, end: u64, mut each: impl FnMut(u64, Option<&[StableId]>)) {
        while let Some(select) = self.selects.front()
            && select.seq < end
        {
            let units = select.units.clone().map(|range| &self.units[range]);
            each(select.seq, units);
            self.selects.pop_front();
        }
        let cut = self
            .selects
            .iter()
            .find_map(|select| select.units.as_ref().map(|range| range.start))
            .unwrap_or(self.units.len());
        self.units.drain(..cut);
        for range in self
            .selects
            .iter_mut()
            .filter_map(|select| select.units.as_mut())
        {
            *range = range.start - cut..range.end - cut;
        }
    }
}

#[cfg(test)]
mod tests;
