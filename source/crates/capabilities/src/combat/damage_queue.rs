use bevy_ecs::resource::Resource;

use crate::combat::damage::Damage;

/// The damage of the running tick, which the pass deals in Resolve: the damage with no source
/// first, then by its source's stable id, then in the order it was queued. Not state: it empties
/// within the tick.
#[derive(Resource, Debug, Default)]
pub(crate) struct DamageQueue {
    entries: Vec<Queued>,
}

/// A damage and its place in the order of the queue.
#[derive(Debug, Clone, Copy)]
struct Queued {
    damage: Damage,
    order: u32,
}

impl DamageQueue {
    pub(crate) fn push(&mut self, damage: Damage) {
        let order = u32::try_from(self.entries.len()).expect("a tick's damage fits u32");
        self.entries.push(Queued { damage, order });
    }

    /// Puts the queue in the order the pass deals it.
    pub(crate) fn sort(&mut self) {
        self.entries
            .sort_unstable_by_key(|queued| (queued.damage.source, queued.order));
    }

    pub(crate) fn get(&self, at: usize) -> Option<Damage> {
        self.entries.get(at).map(|queued| queued.damage)
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}
