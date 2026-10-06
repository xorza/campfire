use bevy_ecs::world::World;
use campfire_common::{Bytes32, StateHash};
use campfire_sim::TypeHash;

use crate::session::Session;

/// The state hashes of a match after each tick, in total and, when it was recorded from a world,
/// by state type: what the replay of its log must give again.
#[derive(Debug, Default)]
pub struct HashTrail {
    /// The state types, in the order of each tick's run of `types`.
    names: Vec<&'static str>,
    types: Vec<Bytes32>,
    totals: Vec<StateHash>,
    scratch: Vec<TypeHash>,
}

impl HashTrail {
    /// A trail of `totals` alone, such as a server's tick hashes.
    pub fn of_totals(totals: Vec<StateHash>) -> HashTrail {
        HashTrail {
            totals,
            ..HashTrail::default()
        }
    }

    /// Records the state of `world`, where a session runs, after a tick.
    pub fn record(&mut self, world: &World) {
        let session = world.resource::<Session>();
        let total = session.state_hash_by_type(world, &mut self.scratch);
        if self.totals.is_empty() {
            self.names = self.scratch.iter().map(|hash| hash.name).collect();
        }
        let names = self.scratch.iter().map(|hash| hash.name);
        assert!(
            names.eq(self.names.iter().copied()),
            "a match keeps its state types"
        );
        self.types.extend(self.scratch.iter().map(|hash| hash.hash));
        self.totals.push(total);
    }

    pub fn totals(&self) -> &[StateHash] {
        &self.totals
    }

    /// The state hash of the tick it recorded last.
    pub fn last(&self) -> StateHash {
        *self.totals.last().expect("a trail that recorded a tick")
    }

    /// How `replayed` differs from it: the first tick of another hash, or another count of
    /// ticks; `None` when it holds the same ticks.
    pub fn difference(&self, replayed: &HashTrail) -> Option<Difference> {
        let tick = self
            .totals
            .iter()
            .zip(&replayed.totals)
            .position(|(live, replayed)| live != replayed);
        if let Some(tick) = tick {
            let types = self.differing_types(replayed, tick);
            return Some(Difference::Tick { tick, types });
        }
        (self.totals.len() != replayed.totals.len()).then_some(Difference::Ticks {
            live: self.totals.len(),
            replayed: replayed.totals.len(),
        })
    }

    /// Asserts that `replayed` holds the same ticks, each of the same hash.
    pub fn assert_same(&self, replayed: &HashTrail) {
        assert_eq!(self.difference(replayed), None);
    }

    /// The state types whose hashes differ between the two at `tick`; none when either holds
    /// only totals.
    fn differing_types(&self, replayed: &HashTrail, tick: usize) -> Vec<&'static str> {
        if self.types.is_empty() || replayed.types.is_empty() || self.names != replayed.names {
            return Vec::new();
        }
        let count = self.names.len();
        let at = tick * count..(tick + 1) * count;
        let pairs = self.types[at.clone()].iter().zip(&replayed.types[at]);
        let names = self.names.iter().zip(pairs);
        names
            .filter(|(_, (live, replayed))| live != replayed)
            .map(|(&name, _)| name)
            .collect()
    }
}

/// How a replay's trail differs from the match's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Difference {
    /// The first tick whose hash differs, and its state types that differ, when both trails hold
    /// them.
    Tick {
        tick: usize,
        types: Vec<&'static str>,
    },
    /// Both agree as far as the shorter goes, and one ran more ticks.
    Ticks { live: usize, replayed: usize },
}

#[cfg(test)]
mod tests {
    use campfire_sim::StateRegistry;

    use super::*;

    /// A trail of ticks whose types `a` and `b` hash as `types` gives, each tick's total the hash
    /// of its two bytes.
    fn trail(types: &[[u8; 2]]) -> HashTrail {
        HashTrail {
            names: vec!["a", "b"],
            types: types
                .iter()
                .flat_map(|&[a, b]| [Bytes32::new([a; 32]), Bytes32::new([b; 32])])
                .collect(),
            totals: types
                .iter()
                .map(|tick| StateRegistry::digest(tick))
                .collect(),
            scratch: Vec::new(),
        }
    }

    #[test]
    fn a_difference_names_the_first_tick_and_its_types_or_the_counts() {
        let live = trail(&[[1, 2], [3, 4], [5, 6]]);
        assert_eq!(live.difference(&trail(&[[1, 2], [3, 4], [5, 6]])), None);
        // Tick 1 differs in `b`; tick 2 in both, which the first difference hides.
        let replayed = trail(&[[1, 2], [3, 9], [7, 8]]);
        assert_eq!(
            live.difference(&replayed),
            Some(Difference::Tick {
                tick: 1,
                types: vec!["b"],
            })
        );
        assert_eq!(
            live.difference(&trail(&[[1, 2], [3, 4]])),
            Some(Difference::Ticks {
                live: 3,
                replayed: 2,
            })
        );
        // A trail of totals alone names no type.
        let totals = HashTrail::of_totals(live.totals().to_vec());
        assert_eq!(totals.difference(&live), None);
        assert_eq!(
            totals.difference(&replayed),
            Some(Difference::Tick {
                tick: 1,
                types: Vec::new(),
            })
        );
    }
}
