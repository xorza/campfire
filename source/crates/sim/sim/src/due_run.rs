use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy_ecs::entity::Entity;
use campfire_common::Tick;

/// The entities a pass visits when their tick comes, as a pass that visits what changed keeps
/// them: each with the one tick it is due at, the earliest first. An entity pushed again keeps
/// the earlier of its two ticks, so a pass may push one entity at each of its due times and get
/// the first, and a push of a tick no earlier than the one it holds adds nothing. An entry is a
/// visit, never a promise: one whose entity
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
    /// Visits `entity` at `at`, or at the tick it is due at when that comes first.
    pub fn push(&mut self, at: Tick, entity: Entity) {
        let index = entity.index_u32() as usize;
        if self.due.len() <= index {
            self.due.resize(index + 1, None);
        }
        if let Some((held, holder)) = self.due[index]
            && holder == entity
            && held <= at
        {
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
        // Tick 5 comes, b's push at 5 kept at its earlier 3: each entity once; 9 stays.
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
        // An entity pushed again at its tick, or a later one, holds one entry: c, due at 4, then
        // 7, comes in tick 4 once. Pushed at an earlier tick, the earlier holds: a, due at 9,
        // then 8, comes in tick 8, and its entry of 9 is passed over.
        run.push(Tick::new(4), c);
        run.push(Tick::new(4), c);
        run.push(Tick::new(7), c);
        assert_eq!(run.entries.len(), 1);
        run.take_due(Tick::new(3), &mut due);
        assert_eq!(due, []);
        run.take_due(Tick::new(4), &mut due);
        assert_eq!(due, [c]);
        assert!(run.entries.is_empty());
        due.clear();
        run.push(Tick::new(9), a);
        run.push(Tick::new(8), a);
        run.take_due(Tick::new(8), &mut due);
        assert_eq!(due, [a]);
        due.clear();
        run.take_due(Tick::new(9), &mut due);
        assert_eq!(due, []);
        assert!(run.entries.is_empty());
    }
}
