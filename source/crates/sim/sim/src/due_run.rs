use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy_ecs::entity::Entity;
use campfire_common::Tick;

/// The entities a pass visits when their tick comes, as a pass that visits what changed keeps
/// them: each with the one tick it is due at, the earliest first. An entity pushed again takes
/// the new tick in place of its old, so the run holds an entry for each entity once, and a push
/// of the tick it holds adds nothing. An entry is a visit, never a promise: one whose entity
/// changed since, or went, costs a visit that finds nothing due. A cache of the pass that owns
/// it, which a restore's new systems build again from their first run, where every part reads as
/// changed; outside the state hash.
#[derive(Debug, Default)]
pub struct DueRun {
    entries: BinaryHeap<Reverse<(Tick, Entity)>>,
    /// The tick each entity is due at, by its index; an entry of the heap that is not its
    /// entity's tick here was moved, and is passed over.
    due: Vec<Option<(Tick, Entity)>>,
}

impl DueRun {
    /// Visits `entity` at `at`, in place of the tick it was due at.
    pub fn push(&mut self, at: Tick, entity: Entity) {
        let index = entity.index_u32() as usize;
        if self.due.len() <= index {
            self.due.resize(index + 1, None);
        }
        if self.due[index] == Some((at, entity)) {
            return;
        }
        self.due[index] = Some((at, entity));
        self.entries.push(Reverse((at, entity)));
    }

    /// Moves each entity due at `now` or before into `due`, out of the run.
    pub fn take_due(&mut self, now: Tick, due: &mut Vec<Entity>) {
        while let Some(&Reverse((at, entity))) = self.entries.peek()
            && at <= now
        {
            self.entries.pop();
            let held = &mut self.due[entity.index_u32() as usize];
            if *held == Some((at, entity)) {
                *held = None;
                due.push(entity);
            }
        }
    }

    /// Forgets every entry, as a pass that visits every entity puts each back.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.due.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_run_gives_each_entity_due_by_now_and_keeps_the_later() {
        let [a, b, c] = [1, 2, 3].map(Entity::from_raw_u32).map(Option::unwrap);
        let mut run = DueRun::default();
        run.push(Tick::new(5), a);
        run.push(Tick::new(3), b);
        run.push(Tick::new(9), c);
        run.push(Tick::new(5), b);
        let mut due = Vec::new();
        run.take_due(Tick::new(2), &mut due);
        assert_eq!(due, []);
        // Tick 5 comes, b's push at 5 in the place of its 3: each entity once; 9 stays.
        run.take_due(Tick::new(5), &mut due);
        due.sort_unstable();
        let mut at_five = [a, b];
        at_five.sort_unstable();
        assert_eq!(due, at_five);
        due.clear();
        run.take_due(Tick::new(8), &mut due);
        assert_eq!(due, []);
        run.take_due(Tick::new(9), &mut due);
        assert_eq!(due, [c]);
        due.clear();
        run.push(Tick::new(1), a);
        run.clear();
        run.take_due(Tick::new(100), &mut due);
        assert_eq!(due, []);
        // An entity pushed again at its tick holds one entry; pushed at another, the new tick
        // takes the old one's place: c, due at 4, then 7, comes in tick 7 once.
        run.push(Tick::new(4), c);
        run.push(Tick::new(4), c);
        assert_eq!(run.entries.len(), 1);
        run.push(Tick::new(7), c);
        run.take_due(Tick::new(6), &mut due);
        assert_eq!(due, []);
        run.take_due(Tick::new(7), &mut due);
        assert_eq!(due, [c]);
        assert!(run.entries.is_empty());
    }
}
