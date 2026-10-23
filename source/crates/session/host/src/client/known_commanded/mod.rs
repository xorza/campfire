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
    /// The selects sent and not acknowledged yet, in the order of their seqs.
    waiting: Vec<WaitingSelect>,
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
        self.known && self.waiting.is_empty() && self.applied == units
    }

    /// Keeps that the input of `seq` holds a whole select of `units`.
    pub fn sent(&mut self, seq: u64, units: &[StableId]) {
        let start = self.units.len();
        self.units.extend_from_slice(units);
        self.wait(seq, Some(start..self.units.len()));
    }

    /// Keeps that the input of `seq` holds a part of a select past one payload.
    pub fn sent_part(&mut self, seq: u64) {
        self.wait(seq, None);
    }

    fn wait(&mut self, seq: u64, units: Option<Range<usize>>) {
        debug_assert!(
            self.waiting.last().is_none_or(|last| last.seq < seq),
            "selects are sent in the order of their seqs"
        );
        self.waiting.push(WaitingSelect { seq, units });
    }

    /// Takes the server's word on the inputs from seq `first` on, each applied in the tick
    /// `applied` gives or never: a whole select applied leaves its units commanded, a part applied
    /// leaves the client not knowing, and one never applied changes nothing.
    pub fn acknowledge(&mut self, first: u64, applied: &[Option<Tick>]) {
        let acknowledged = |seq: u64| {
            let at = usize::try_from(seq.checked_sub(first)?).ok()?;
            applied.get(at).copied()
        };
        let Self {
            applied: held,
            known,
            waiting,
            units,
        } = self;
        waiting.retain(|select| {
            let Some(tick) = acknowledged(select.seq) else {
                return true;
            };
            if tick.is_some() {
                match &select.units {
                    Some(range) => {
                        held.clear();
                        held.extend_from_slice(&units[range.clone()]);
                        *known = true;
                    }
                    None => *known = false,
                }
            }
            false
        });
        if waiting.is_empty() {
            units.clear();
        }
    }

    /// Forgets all it knew, as a client whose link failed does: the server's word on its inputs
    /// no longer comes.
    pub fn clear(&mut self) {
        self.applied.clear();
        self.known = false;
        self.waiting.clear();
        self.units.clear();
    }
}

#[cfg(test)]
mod tests;
