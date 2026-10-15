use std::cmp::Reverse;
use std::collections::BinaryHeap;

use bevy_ecs::entity::Entity;
use campfire_common::Tick;

/// The entities a pass visits when their tick comes, as a pass that visits what changed keeps
/// them: each with the tick it is due at, the earliest first. An entry is a visit, never a
/// promise: one whose entity changed since, or went, costs a visit that finds nothing due. A
/// cache of the pass that owns it, which a restore's new systems build again from their first
/// run, where every part reads as changed; outside the state hash.
#[derive(Debug, Default)]
pub struct DueRun {
    entries: BinaryHeap<Reverse<(Tick, Entity)>>,
}

impl DueRun {
    /// Visits `entity` at `at`.
    pub fn push(&mut self, at: Tick, entity: Entity) {
        self.entries.push(Reverse((at, entity)));
    }

    /// Moves each entity due at `now` or before into `due`, out of the run.
    pub fn take_due(&mut self, now: Tick, due: &mut Vec<Entity>) {
        while let Some(&Reverse((at, entity))) = self.entries.peek()
            && at <= now
        {
            self.entries.pop();
            due.push(entity);
        }
    }

    /// Forgets every entry, as a pass that visits every entity puts each back.
    pub fn clear(&mut self) {
        self.entries.clear();
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
        // Ticks 3 and 5 come, each entry once, the earlier tick first; 9 stays.
        run.take_due(Tick::new(5), &mut due);
        let mut at_five = [a, b];
        at_five.sort_unstable();
        due[1..].sort_unstable();
        assert_eq!(due, [b, at_five[0], at_five[1]]);
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
    }
}
