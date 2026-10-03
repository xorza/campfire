use std::collections::VecDeque;

use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::combat::damage::Damage;
use crate::combat::heal::Heal;

/// The damage and heals of the running tick, which the pass applies in Resolve: those with no
/// source first, then by their source's stable id, then in the order they were queued; what the
/// pass queues joins the end, but for a heal, which it deals next. Not state: it empties within
/// the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct PassQueue {
    entries: Vec<Queued>,
    /// The next entry of `entries` the pass deals.
    at: usize,
    /// The heals the running pass gave, from leech and from hooks, in the order given.
    given: VecDeque<Heal>,
    running: bool,
}

/// A damage or a heal of the pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PassEntry {
    Damage(Damage),
    Heal(Heal),
}

/// An entry and its place in the order of the queue.
#[derive(Debug, Clone, Copy)]
struct Queued {
    entry: PassEntry,
    order: u32,
}

impl PassQueue {
    pub(crate) fn push_damage(&mut self, damage: Damage) {
        self.push(PassEntry::Damage(damage));
    }

    /// Queues `heal`: while the pass runs, to be dealt before the pass's next entry, after the
    /// heals given before it.
    pub(crate) fn push_heal(&mut self, heal: Heal) {
        if self.running {
            self.given.push_back(heal);
        } else {
            self.push(PassEntry::Heal(heal));
        }
    }

    fn push(&mut self, entry: PassEntry) {
        let order = u32::try_from(self.entries.len()).expect("a tick's damage fits u32");
        self.entries.push(Queued { entry, order });
    }

    /// Puts the queue in the order the pass applies it, and starts the pass.
    pub(crate) fn begin(&mut self) {
        debug_assert!(!self.running, "one pass at a time");
        self.entries
            .sort_unstable_by_key(|queued| (queued.entry.source(), queued.order));
        self.running = true;
    }

    /// The entry the pass deals next: a heal the pass gave, or else the next entry of the queue.
    pub(crate) fn next(&mut self) -> Option<PassEntry> {
        if let Some(heal) = self.given.pop_front() {
            return Some(PassEntry::Heal(heal));
        }
        let queued = self.entries.get(self.at)?;
        self.at += 1;
        Some(queued.entry)
    }

    /// Ends the pass, and empties the queue.
    pub(crate) fn end(&mut self) {
        debug_assert!(self.given.is_empty(), "the pass deals every heal it gave");
        self.running = false;
        self.clear();
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.given.clear();
        self.at = 0;
    }
}

impl PassEntry {
    const fn source(self) -> Option<StableId> {
        match self {
            PassEntry::Damage(damage) => damage.source,
            PassEntry::Heal(heal) => heal.source,
        }
    }
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::combat::pass_queue::{PassEntry, PassQueue};

    impl PassQueue {
        /// The entry at `at` of the queue as it was queued, or as the pass sorted it.
        pub(crate) fn get(&self, at: usize) -> Option<PassEntry> {
            self.entries.get(at).map(|queued| queued.entry)
        }
    }
}
