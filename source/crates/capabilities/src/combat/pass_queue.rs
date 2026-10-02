use bevy_ecs::resource::Resource;
use campfire_sim::StableId;

use crate::combat::damage::Damage;
use crate::combat::heal::Heal;

/// The damage and heals of the running tick, which the pass applies in Resolve: those with no
/// source first, then by their source's stable id, then in the order they were queued; what the
/// pass queues joins the end. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct PassQueue {
    entries: Vec<Queued>,
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

    pub(crate) fn push_heal(&mut self, heal: Heal) {
        self.push(PassEntry::Heal(heal));
    }

    fn push(&mut self, entry: PassEntry) {
        let order = u32::try_from(self.entries.len()).expect("a tick's damage fits u32");
        self.entries.push(Queued { entry, order });
    }

    /// Puts the queue in the order the pass applies it.
    pub(crate) fn sort(&mut self) {
        self.entries
            .sort_unstable_by_key(|queued| (queued.entry.source(), queued.order));
    }

    pub(crate) fn get(&self, at: usize) -> Option<PassEntry> {
        self.entries.get(at).map(|queued| queued.entry)
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
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
