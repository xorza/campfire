use std::mem;
use std::ops::ControlFlow;

use bevy_ecs::component::{Component, Mutable};
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Tick};
use campfire_sim::StableId;

use crate::script_host::ScriptId;
use crate::scripts::call_start::CallStart;
use crate::scripts::ctx::Ctx;
use crate::scripts::hook::{Hook, HookDecl};
use crate::scripts::hook_set::HookSet;
use crate::scripts::mode_scripts::ListPlace;
use crate::scripts::script_batch::{Answered, ScriptBatch};

/// A queue of events the mode's scripts answer by their hooks, in order, the state of the
/// capability that produces them: an event whose call finds the mode's pool spent waits, with
/// those after it, for a later tick, which runs it in the scripts that did not answer it
/// (06-script-api.md, A queue of mode events answers itself; 03-game-scripting.md, An event waits
/// in the scripts that did not answer it).
pub trait ModeEvents: Resource + Component<Mutability = Mutable> + Default {
    /// The hooks that answer its events: the mode answers it when one of its scripts defines one.
    const HOOKS: &'static [Hook];

    /// Moves to its end the events of the tick that `world` holds apart, as the stage begins;
    /// it writes nothing when there is none.
    fn arrive(world: &mut World);

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// How many of the mode's scripts answered its first event, which waits for the rest.
    fn partial(&self) -> u8;

    /// Answers its events through `answer`, first to last, up to the first whose call waits:
    /// how many it answered.
    fn answer(&self, answer: &mut ModeAnswer<'_, '_>) -> usize;

    /// Removes the first `count` events, whose calls ran, and records that `partial` of the
    /// mode's scripts answered the event then first, which waits for the rest.
    fn answered(&mut self, count: usize, partial: u8);

    /// Moves the events of `later` to its end, leaving it empty.
    fn append(&mut self, later: &mut Self);

    /// What the events from the `answered`-th on hold in `world` while they wait, as a death
    /// keeps its unit, and what the answered ones let go.
    fn hold(&self, answered: usize, world: &mut World);
}

/// A script of a mode's list, and the hooks of the mode's role it defines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedScript {
    pub script: ScriptId,
    pub hooks: HookSet,
}

/// The mode's scripts as a stage answers events through them: the batch of the stage's calls,
/// the scripts in their list's order, the time the calls run at, and how many of the scripts
/// answered the first event in an earlier tick.
#[derive(Debug)]
pub struct ModeAnswer<'a, 'w> {
    batch: &'a mut ScriptBatch<'w>,
    scripts: &'a [ListedScript],
    now: Tick,
    /// How many scripts the first call passes, as they answered it before.
    resume: u8,
    /// How many scripts answered the call that waits.
    partial: u8,
}

impl<'a, 'w> ModeAnswer<'a, 'w> {
    /// The answer through `scripts` at `now`, of whose first event `resume` of the scripts that
    /// define its hook answered it before.
    pub const fn new(
        batch: &'a mut ScriptBatch<'w>,
        scripts: &'a [ListedScript],
        now: Tick,
        resume: u8,
    ) -> Self {
        Self {
            batch,
            scripts,
            now,
            resume,
            partial: 0,
        }
    }

    /// The `ctx` of the calls.
    pub const fn ctx(&self) -> &Ctx {
        self.batch.ctx()
    }

    /// How many of the scripts that define its hook answered the call that waits.
    pub const fn partial(&self) -> u8 {
        self.partial
    }

    /// Answers with the hook `H` in each script that defines it, for `id` if any, from the mode's
    /// pool, as `call_from` does.
    pub fn call<H: HookDecl>(
        &mut self,
        id: Option<StableId>,
        args: impl Fn(&Ctx) -> Option<H::Args>,
    ) -> ControlFlow<()> {
        self.call_from::<H>(id, None, args)
    }

    /// Answers with the hook `H` in each script that defines it, in the list's order, for `id`
    /// if any, from the pool of `payer`, the player whose input it answers, or the mode's: with
    /// the arguments `args` gives each, or with no call when it gives none, as for a unit gone by
    /// then. The first call of the answer runs only in the scripts that did not answer it before.
    /// A failure is recorded; `Break` when a call waits, which `partial` then counts the scripts
    /// before.
    pub fn call_from<H: HookDecl>(
        &mut self,
        id: Option<StableId>,
        payer: Option<PlayerSlot>,
        args: impl Fn(&Ctx) -> Option<H::Args>,
    ) -> ControlFlow<()> {
        let hook = Hook::of::<H>();
        let skip = usize::from(mem::take(&mut self.resume));
        let defining = self.scripts.iter().enumerate();
        let defining = defining.filter(|(_, listed)| listed.hooks.contains(hook));
        for (answered, (at, listed)) in defining.enumerate().skip(skip) {
            let place = ListPlace::new(at).expect("a mode's list is within its limit");
            match self.answer_in::<H>(place, listed.script, id, payer, &args) {
                Outcome::Answered => {}
                Outcome::NoCall => return ControlFlow::Continue(()),
                Outcome::Waits => {
                    self.partial =
                        u8::try_from(answered).expect("a script that waits has its place");
                    return ControlFlow::Break(());
                }
            }
        }
        ControlFlow::Continue(())
    }

    /// Answers with the hook `H` in the script at `place` alone, when it defines it, as
    /// `call_from` does, from the mode's pool.
    pub fn call_in<H: HookDecl>(
        &mut self,
        place: ListPlace,
        id: Option<StableId>,
        args: impl Fn(&Ctx) -> Option<H::Args>,
    ) -> ControlFlow<()> {
        let Some(listed) = self.scripts.get(place.index()) else {
            return ControlFlow::Continue(());
        };
        if !listed.hooks.contains(Hook::of::<H>()) {
            return ControlFlow::Continue(());
        }
        match self.answer_in::<H>(place, listed.script, id, None, &args) {
            Outcome::Answered | Outcome::NoCall => ControlFlow::Continue(()),
            Outcome::Waits => ControlFlow::Break(()),
        }
    }

    /// Runs `H` in `script`, at `place` in the list, with the arguments `args` gives.
    fn answer_in<H: HookDecl>(
        &mut self,
        place: ListPlace,
        script: ScriptId,
        id: Option<StableId>,
        payer: Option<PlayerSlot>,
        args: &impl Fn(&Ctx) -> Option<H::Args>,
    ) -> Outcome {
        let Some(args) = args(self.batch.ctx()) else {
            return Outcome::NoCall;
        };
        let start = CallStart {
            payer,
            ..CallStart::listed(place, self.now)
        };
        let answered = self.batch.answer(start, Hook::of::<H>(), id, |batch| {
            batch.call::<H>(script, args).map(drop)
        });
        match answered {
            Answered::Ran(()) | Answered::Failed => Outcome::Answered,
            Answered::Waits => Outcome::Waits,
        }
    }
}

/// What a call of one script did with an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    /// It ran, or failed, which the batch recorded.
    Answered,
    /// Its arguments are gone, as its unit is: no script answers the event.
    NoCall,
    /// It found the pool spent.
    Waits,
}
