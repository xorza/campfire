use std::collections::VecDeque;

use bevy_ecs::resource::Resource;
use campfire_common::Tick;
use campfire_declarations::{Role, Scratch};
use campfire_sim::StableId;
use serde::{Deserialize, Serialize};

use crate::damage::Damage;
use crate::heal::Heal;

/// The damage and heals of the running tick, which the pass applies in Resolve: the next tick's
/// damage the tick before queued after its pass first, in the order queued, then those with no
/// source, then by their source's stable id, then in the order they were queued; what the pass
/// queues joins the end, but for a heal, which it deals next. Each push names its tick, which
/// must not be one whose pass ran: what comes after the pass is the next tick's damage. Not
/// state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct PassQueue {
    entries: Vec<Queued>,
    /// The next entry of `entries` the pass deals.
    at: usize,
    /// The heals the running pass gave, from leech and from hooks, in the order given.
    given: VecDeque<Heal>,
    running: bool,
    /// The tick whose pass ran last.
    dealt: Option<Tick>,
}

/// A damage or a heal of the pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PassEntry {
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
    /// Queues `damage` for the pass of `now`, which has not run.
    pub(crate) fn push_damage(&mut self, damage: Damage, now: Tick) {
        self.push(PassEntry::Damage(damage), now);
    }

    /// Queues `heal` for the pass of `now`, which has not run: while the pass runs, to be dealt
    /// before the pass's next entry, after the heals given before it.
    pub(crate) fn push_heal(&mut self, heal: Heal, now: Tick) {
        if self.running {
            self.given.push_back(heal);
        } else {
            self.push(PassEntry::Heal(heal), now);
        }
    }

    /// Queues `entry` for the pass of `now`, which has not run.
    fn push(&mut self, entry: PassEntry, now: Tick) {
        debug_assert!(
            self.open_at(now),
            "an entry after the tick's pass is the next tick's damage"
        );
        let order = u32::try_from(self.entries.len()).expect("a tick's damage fits u32");
        self.entries.push(Queued { entry, order });
    }

    /// Whether an entry of tick `now` reaches its pass: the pass of `now` has not run.
    pub(crate) fn open_at(&self, now: Tick) -> bool {
        self.dealt != Some(now)
    }

    /// Puts the queue in the order the pass applies it, the entries `carried` from the tick
    /// before first, and starts the pass.
    pub(crate) fn begin(&mut self, carried: impl IntoIterator<Item = PassEntry>) {
        debug_assert!(!self.running, "one pass at a time");
        self.entries
            .sort_unstable_by_key(|queued| (queued.entry.source(), queued.order));
        let carried = carried.into_iter().map(|entry| Queued { entry, order: 0 });
        self.entries.splice(0..0, carried);
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

    /// Ends the pass of `now`, and empties the queue.
    pub(crate) fn end(&mut self, now: Tick) {
        debug_assert!(self.given.is_empty(), "the pass deals every heal it gave");
        self.running = false;
        self.dealt = Some(now);
        self.empty();
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn empty(&mut self) {
        self.entries.clear();
        self.given.clear();
        self.at = 0;
    }
}

impl PassEntry {
    const fn source(self) -> Option<StableId> {
        match self {
            Self::Damage(damage) => damage.source,
            Self::Heal(heal) => heal.source,
        }
    }
}

impl Scratch for PassQueue {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        !self.running && self.entries.is_empty() && self.given.is_empty() && self.at == 0
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::pass_queue::{PassEntry, PassQueue};

    impl PassQueue {
        /// The entry at `at` of the queue as it was queued, or as the pass sorted it.
        pub fn get(&self, at: usize) -> Option<PassEntry> {
            self.entries.get(at).map(|queued| queued.entry)
        }

        /// Empties the queue, as a test that strikes again starts from none.
        pub fn clear(&mut self) {
            self.empty();
        }
    }
}
