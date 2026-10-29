use std::ops::ControlFlow;
use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_combat::{CalcDamage, CalcHeal, Damage, DamageHandle, Heal, HealHandle};
use campfire_common::{PlayerSlot, Tick};
use campfire_math::Num;
use campfire_script::{
    CallError, CallStart, CapabilityRefusal, Ctx, Hook, HookDecl, ListPlace, ModeAnswer,
    ScriptBatch, ScriptsWorld,
};
use campfire_sim::{SimTick, StableId};

use crate::error::ModeRefusal;
use crate::mode_ctx::ModeCtx;
use crate::mode_spec::ModeSpec;

/// The mode calls of one batch, at one time.
#[derive(Debug)]
pub struct Calls<'a, 'w> {
    pub(crate) batch: &'a mut ScriptBatch<'w>,
    pub(crate) ctx: Ctx,
    book: Arc<ModeSpec>,
    now: Tick,
}

impl<'w> Calls<'_, 'w> {
    /// Runs `calls` in one script batch of `world` at the time `now`, and gives what `calls`
    /// gave.
    pub(crate) fn scope<T>(
        world: &mut World,
        now: Tick,
        calls: impl FnOnce(&mut Calls<'_, '_>) -> T,
    ) -> T {
        world.script_batch(|batch| {
            let ctx = batch.ctx().clone();
            let book = ctx.match_mode_spec();
            calls(&mut Calls {
                batch,
                ctx,
                book,
                now,
            })
        })
    }

    /// The match's mode.
    pub(crate) fn book(&self) -> Arc<ModeSpec> {
        Arc::clone(&self.book)
    }

    /// The mode's scripts as the batch's events are answered through them, of whose first event
    /// `resume` of the scripts that define its hook answered it before.
    pub(crate) fn mode_answer(&mut self, resume: u8) -> ModeAnswer<'_, 'w> {
        let scripts = &self.book.schema.scripts;
        ModeAnswer::new(self.batch, scripts, self.now, resume)
    }

    /// Runs the hook `H` with the arguments `args` gives, from the mode's budget, in each of the
    /// mode's scripts that defines it, in its list's order: on success a call's state, choices
    /// and ids commit and its effects apply; the first failure ends the run, which changes
    /// nothing of its call.
    pub(crate) fn run<H: HookDecl>(&mut self, args: impl Fn() -> H::Args) -> Result<(), CallError> {
        let book = self.book();
        let hook = Hook::of::<H>();
        for (at, listed) in book.schema.scripts.iter().enumerate() {
            if !listed.hooks.contains(hook) {
                continue;
            }
            let place = ListPlace::new(at).expect("a mode's list is within its limit");
            let start = CallStart::listed(place, self.now);
            let script = listed.script;
            self.batch
                .run(start, |batch| batch.call::<H>(script, args()).map(drop))?;
        }
        Ok(())
    }

    /// Answers with the hook `H` and `args`, for `id` if any, from the budget of `payer`, the
    /// player whose input it answers, or the mode's, as `ModeAnswer::call_from` does: `Break`
    /// when a call waits, so what the caller answers in a later tick waits, from this call on.
    pub(crate) fn answer<H: HookDecl>(
        &mut self,
        id: Option<StableId>,
        payer: Option<PlayerSlot>,
        args: H::Args,
    ) -> ControlFlow<()>
    where
        H::Args: Clone,
    {
        self.mode_answer(0)
            .call_from::<H>(id, payer, |_| Some(args.clone()))
    }

    /// Answers with the hook `H` and the arguments `args` gives of the call's `ctx` in the script
    /// at `place` of the mode's list alone, as a timer of it runs, from the mode's budget: `Break`
    /// when the call waits.
    pub(crate) fn answer_in<H: HookDecl>(
        &mut self,
        place: ListPlace,
        args: impl Fn(&Ctx) -> H::Args,
    ) -> ControlFlow<()> {
        self.mode_answer(0)
            .call_in::<H>(place, None, |ctx| Some(args(ctx)))
    }

    /// `calc_damage` of `damage` in `batch`, its `ctx` pure: the number it returns, an integer
    /// as a number; none when no script of the mode defines `calc_damage`.
    pub(crate) fn weigh_damage(
        batch: &mut ScriptBatch<'_>,
        damage: Damage,
    ) -> Result<Option<Num>, CallError> {
        let handle = DamageHandle::new(damage, batch.ctx().view().clone());
        Calls::amount::<CalcDamage>(batch, |ctx| (ctx, handle))
    }

    /// The amount of `heal` before the heal scale: what the mode's `calc_heal` returns for it,
    /// a pure call in `batch`; an error when the call fails or returns no number, and none when
    /// no script of the mode defines `calc_heal`.
    pub(crate) fn weigh_heal(
        batch: &mut ScriptBatch<'_>,
        heal: Heal,
    ) -> Result<Option<Num>, CallError> {
        let handle = HealHandle::new(heal, batch.ctx().view().clone());
        Calls::amount::<CalcHeal>(batch, |ctx| (ctx, handle))
    }

    /// The number the mode's pure hook `H` returns for the arguments `args` gives with the
    /// batch's `ctx`, in `batch`, from the one script of the mode that defines it; none when none
    /// does.
    fn amount<H: HookDecl>(
        batch: &mut ScriptBatch<'_>,
        args: impl FnOnce(Ctx) -> H::Args,
    ) -> Result<Option<Num>, CallError> {
        let ctx = batch.ctx().clone();
        let Some((place, script)) = ctx.match_mode_spec().schema.pure_script(Hook::of::<H>())
        else {
            return Ok(None);
        };
        let now = batch.world().resource::<SimTick>().start();
        let value = batch.run(CallStart::pure(place, now), |batch| {
            batch.call_pure::<H>(script, args(ctx))
        });
        let value = value?;
        let amount = match value.as_int() {
            Some(int) => Num::from_int(int),
            None => value.try_cast::<Num>(),
        };
        let amount = amount.ok_or_else(|| CallError::Api(ModeRefusal::NotAnAmount.refused()))?;
        Ok(Some(amount))
    }
}
