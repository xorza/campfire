use bevy_ecs::world::World;
use campfire_math::{Num, Rng, RngStream};
use campfire_sim::{RngOpener, SimRng, StableId};

use crate::scripts::call_part::{CallPart, Commit};
use crate::scripts::call_start::CallStart;
use crate::scripts::error::CallError;

/// The stream of an acting unit's draws, and of the mode's own calls'.
const UNIT_DRAWS: RngStream = RngStream::new("script.draw");
const MODE_DRAWS: RngStream = RngStream::new("script.mode");

/// The draws of `ctx.chance` and `ctx.pick`: the sequences the running tick opened, one for each
/// acting unit and one for the mode's own calls, kept from call to call, so each draw takes the
/// next words of its drawer's sequence. Not state: it starts again with each start of a tick.
#[derive(Debug, Default)]
pub struct ScriptCall {
    /// The running tick's opener.
    opener: Option<RngOpener>,
    /// Whose sequence the running call draws on: its acting unit's, or none for the mode's.
    drawer: Option<StableId>,
    /// The sequences the tick opened, sorted by drawer: none for the mode.
    open: Vec<(Option<StableId>, Rng)>,
}

impl CallPart for ScriptCall {
    const COMMITS: Commit = Commit::AfterEffects;

    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        let opener = world.resource::<SimRng>().opener();
        if self.opener != Some(opener) {
            self.open.clear();
            self.opener = Some(opener);
        }
        self.drawer = start.acting;
        Ok(())
    }

    /// The draws are no state, so a call commits nothing of them.
    fn commit(&mut self, _: &mut World) {}
}

impl ScriptCall {
    /// Whether a draw with probability `probability`, from 0 to 1, comes true.
    pub(crate) fn chance(&mut self, probability: Num) -> bool {
        debug_assert!((Num::ZERO..=Num::ONE).contains(&probability));
        self.sequence().chance(probability)
    }

    /// An index into `len` entries, each as likely; `len` is not zero.
    pub(crate) fn pick(&mut self, len: usize) -> usize {
        debug_assert!(len > 0, "a pick from no entries");
        self.sequence().pick(len)
    }

    /// The running call's drawer's sequence, opened at its first draw in the tick.
    fn sequence(&mut self) -> &mut Rng {
        let drawer = self.drawer;
        let at = match self.open.binary_search_by_key(&drawer, |&(held, _)| held) {
            Ok(at) => at,
            Err(at) => {
                let opener = self.opener.expect("a call began");
                let rng = match drawer {
                    Some(unit) => opener.open(UNIT_DRAWS, unit),
                    None => opener.open(MODE_DRAWS, StableId::MATCH),
                };
                self.open.insert(at, (drawer, rng));
                at
            }
        };
        &mut self.open[at].1
    }
}

#[cfg(test)]
mod tests {
    use std::array;

    use campfire_common::SegmentSeed;
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn each_drawer_draws_on_its_own_sequence_whatever_the_order() {
        let opener = SimRng::new(SegmentSeed::new([7; 32])).opener();
        let mut ids = IdAllocator::default();
        let units: [StableId; 4] = array::from_fn(|_| ids.allocate());
        let mut call = ScriptCall {
            opener: Some(opener),
            ..ScriptCall::default()
        };
        // Drawers out of id order, the mode among them, and each again later.
        let order = [
            Some(3),
            None,
            Some(1),
            Some(3),
            Some(0),
            None,
            Some(1),
            Some(3),
        ];
        let mut drawn: Vec<(Option<usize>, usize)> = Vec::new();
        for drawer in order {
            call.drawer = drawer.map(|at| units[at]);
            drawn.push((drawer, call.pick(1000)));
        }
        // Each drawer's picks are its own sequence's, in its own order.
        for drawer in [None, Some(0), Some(1), Some(3)] {
            let mut rng = match drawer {
                Some(at) => opener.open(UNIT_DRAWS, units[at]),
                None => opener.open(MODE_DRAWS, StableId::MATCH),
            };
            let own = drawn.iter().filter(|&&(by, _)| by == drawer);
            for &(_, pick) in own {
                assert_eq!(pick, rng.pick(1000), "{drawer:?}");
            }
        }
        let held: Vec<_> = call.open.iter().map(|&(drawer, _)| drawer).collect();
        assert_eq!(held, [None, Some(units[0]), Some(units[1]), Some(units[3])]);
    }
}
