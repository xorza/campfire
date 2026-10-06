use bevy_ecs::world::World;
use campfire_math::{Num, Rng, RngOpener, RngStream};
use campfire_script::rhai::Dynamic;
use campfire_sim::{SimRng, StableId};

use crate::scripts::call_part::CallPart;
use crate::scripts::call_start::CallStart;
use crate::scripts::error::CallError;
use crate::scripts::script_role::ScriptRole;

/// The stream of an acting unit's draws, and of the mode's own calls'.
const UNIT_DRAWS: RngStream = RngStream::new("script.draw");
const MODE_DRAWS: RngStream = RngStream::new("script.mode");

/// The draws of `ctx.chance` and `ctx.pick`: the sequences the running tick opened, one for each
/// acting unit and one for the mode's own calls, kept from call to call, so each draw takes the
/// next words of its drawer's sequence. Not state: it starts again with each start of a tick.
#[derive(Debug, Default)]
pub(crate) struct ScriptsCall {
    /// The running tick's opener.
    opener: Option<RngOpener>,
    /// Whose sequence the running call draws on: its acting unit's, or none for the mode's.
    drawer: Option<StableId>,
    /// The sequences the tick opened, by drawer: none for the mode.
    open: Vec<(Option<StableId>, Rng)>,
}

impl CallPart for ScriptsCall {
    fn begin(&mut self, world: &World, start: &CallStart) -> Result<(), CallError> {
        let opener = world.resource::<SimRng>().opener();
        if self.opener != Some(opener) {
            self.open.clear();
            self.opener = Some(opener);
        }
        self.drawer = start.acting;
        Ok(())
    }

    fn param_named(&self, _: Option<ScriptRole>, _: &str) -> Option<Dynamic> {
        None
    }

    fn apply(&mut self, _: &mut World) {}
}

impl ScriptsCall {
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
        if let Some(at) = self.open.iter().position(|(held, _)| *held == drawer) {
            return &mut self.open[at].1;
        }
        let opener = self.opener.expect("a call began");
        let rng = match drawer {
            Some(unit) => opener.open(UNIT_DRAWS, unit.get()),
            None => opener.open(MODE_DRAWS, 0),
        };
        self.open.push((drawer, rng));
        &mut self.open.last_mut().expect("the sequence just opened").1
    }
}
