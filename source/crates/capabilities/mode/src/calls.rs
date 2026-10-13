use std::ops::ControlFlow;

use campfire_combat::{Damage, DamageHandle, Heal, HealHandle};
use campfire_common::Tick;
use campfire_core::{ApiError, CallError, Ctx, Hook, Pool, ScriptArgs, ScriptBatch, ScriptValue};
use campfire_math::Num;
use campfire_script::ScriptError;
use campfire_sim::{SimTick, StableId};

use crate::choices::Choices;
use crate::mode_call::ModeCall;
use crate::mode_spec::ModeSpec;

/// The mode calls of one batch, at one time.
#[derive(Debug)]
pub struct Calls<'a, 'w> {
    pub(crate) batch: &'a mut ScriptBatch<'w>,
    pub(crate) ctx: Ctx,
    pub(super) now: Tick,
}

impl Calls<'_, '_> {
    /// The match's mode.
    pub(crate) fn book(&self) -> &ModeSpec {
        ModeSpec::of_match(&self.ctx)
    }

    /// Runs `hook` with `args` from `pool`: on success its state, choices and ids commit and its
    /// effects apply; on failure nothing changes.
    pub(crate) fn run(
        &mut self,
        pool: Pool,
        hook: Hook,
        args: impl ScriptArgs,
    ) -> Result<(), ScriptError> {
        self.begin(false);
        let script = self.book().schema.script;
        drop(self.batch.call(pool, script, hook, args)?);
        self.commit();
        Ok(())
    }

    /// Runs `hook` with `args` from `pool` as `run` does, and records a failure, for `id` if
    /// any. `Break`, with the error unrecorded, when the call found the pool spent: what the
    /// caller answers in a later tick waits, from this call on.
    pub(crate) fn answer(
        &mut self,
        id: Option<StableId>,
        pool: Pool,
        hook: Hook,
        args: impl ScriptArgs,
    ) -> ControlFlow<CallError> {
        match self.run(pool, hook, args) {
            Ok(()) => ControlFlow::Continue(()),
            Err(ScriptError::TickBudget) => {
                ControlFlow::Break(CallError::from_script(ScriptError::TickBudget))
            }
            Err(error) => {
                self.batch.record(id, hook, CallError::from_script(error));
                ControlFlow::Continue(())
            }
        }
    }

    /// `calc_damage` of `damage` in `batch`, its `ctx` pure: the number it returns, an integer
    /// as a number.
    pub(crate) fn weigh_damage(
        batch: &mut ScriptBatch<'_>,
        damage: Damage,
    ) -> Result<Num, CallError> {
        let handle = DamageHandle::new(damage, batch.ctx().view().clone());
        Calls::amount(batch, Hook::CalcDamage, ScriptValue::of(handle))
    }

    /// The amount of `heal` before the heal scale: what the mode's `calc_heal` returns for it,
    /// a pure call in `batch`; an error when the call fails or returns no number.
    pub(crate) fn weigh_heal(batch: &mut ScriptBatch<'_>, heal: Heal) -> Result<Num, CallError> {
        let handle = HealHandle::new(heal, batch.ctx().view().clone());
        Calls::amount(batch, Hook::CalcHeal, ScriptValue::of(handle))
    }

    /// The number the mode's pure `hook` returns for `handle`, in `batch`.
    fn amount(
        batch: &mut ScriptBatch<'_>,
        hook: Hook,
        handle: ScriptValue,
    ) -> Result<Num, CallError> {
        let now = batch.world().resource::<SimTick>().start();
        let ctx = batch.ctx().clone();
        let mut calls = Calls {
            batch,
            ctx: ctx.clone(),
            now,
        };
        calls.begin(true);
        let script = calls.book().schema.script;
        let returned = calls.batch.call_pure(script, hook, (ctx, handle));
        let value = returned.map_err(CallError::from_script)?;
        let amount = match value.as_int() {
            Some(int) => Num::from_int(int),
            None => value.try_cast::<Num>(),
        };
        amount.ok_or(CallError::Api(ApiError::NotAnAmount))
    }

    /// Starts a mode call on the mode's state as it stands; `pure` for a hook whose `ctx` only
    /// reads.
    fn begin(&mut self, pure: bool) {
        let world = self.batch.world();
        self.ctx.frame().begin_mode(world, pure, self.now);
    }

    /// Commits the call's state and choices, then applies its effects in order.
    fn commit(&mut self) {
        let world = self.batch.world();
        {
            let mut frame = self.ctx.frame();
            let call = ModeCall::of_mut(&mut frame);
            call.commit_state(world);
            // A write marks the choices changed, so a call that changed none writes nothing.
            if *world.resource::<Choices>() != call.choices {
                world.resource_mut::<Choices>().clone_from(&call.choices);
            }
        }
        self.ctx.apply(world);
    }
}
