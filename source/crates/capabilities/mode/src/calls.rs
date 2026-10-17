use std::ops::ControlFlow;
use std::sync::Arc;

use bevy_ecs::world::World;
use campfire_combat::{Damage, DamageHandle, Heal, HealHandle};
use campfire_common::Tick;
use campfire_math::Num;
use campfire_script::{
    CallError, CallStart, CapabilityRefusal, Ctx, Hook, HookCall, Pool, ScriptArgs, ScriptBatch,
    ScriptRole, ScriptValue, ScriptsWorld,
};
use campfire_sim::{SimTick, StableId};

use crate::error::ModeRefusal;
use crate::mode_call::ModeCall;
use crate::mode_ctx::ModeCtx;
use crate::mode_spec::ModeSpec;

/// The mode calls of one batch, at one time.
#[derive(Debug)]
pub struct Calls<'a, 'w> {
    pub(crate) batch: &'a mut ScriptBatch<'w>,
    pub(crate) ctx: Ctx,
    now: Tick,
}

impl Calls<'_, '_> {
    /// Runs `calls` in one script batch of `world` at the time `now`, and gives what `calls`
    /// gave.
    pub(crate) fn scope<T>(
        world: &mut World,
        now: Tick,
        calls: impl FnOnce(&mut Calls<'_, '_>) -> T,
    ) -> T {
        world.script_batch(|batch| {
            let ctx = batch.ctx().clone();
            ModeCall::of(&ctx.frame()).debug_assert_no_batch();
            let given = calls(&mut Calls {
                batch,
                ctx: ctx.clone(),
                now,
            });
            ModeCall::of_mut(&mut ctx.frame()).end_batch();
            given
        })
    }

    /// The match's mode.
    pub(crate) fn book(&self) -> Arc<ModeSpec> {
        self.ctx.match_mode_spec()
    }

    /// Runs `hook` with `args` from `pool`: on success its state, choices and ids commit and its
    /// effects apply; on failure nothing changes.
    pub(crate) fn run(
        &mut self,
        pool: Pool,
        hook: Hook,
        args: impl ScriptArgs,
    ) -> Result<(), CallError> {
        let script = self.book().schema.script;
        let start = CallStart::mode(ScriptRole::Mode, self.now);
        self.batch.run(start, |batch| {
            batch.call(pool, HookCall { script, hook, args }).map(drop)
        })
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
            Err(CallError::PoolSpent) => ControlFlow::Break(CallError::PoolSpent),
            Err(error) => {
                self.batch.record(id, hook, &error);
                ControlFlow::Continue(())
            }
        }
    }

    /// `calc_damage` of `damage` in `batch`, its `ctx` pure: the number it returns, an integer
    /// as a number; none when the mode's script defines no `calc_damage`.
    pub(crate) fn weigh_damage(
        batch: &mut ScriptBatch<'_>,
        damage: Damage,
    ) -> Option<Result<Num, CallError>> {
        if !Calls::defines(batch, Hook::CalcDamage) {
            return None;
        }
        let handle = DamageHandle::new(damage, batch.ctx().view().clone());
        Some(Calls::amount(
            batch,
            Hook::CalcDamage,
            ScriptValue::of(handle),
        ))
    }

    /// The amount of `heal` before the heal scale: what the mode's `calc_heal` returns for it,
    /// a pure call in `batch`; an error when the call fails or returns no number, and none when
    /// the mode's script defines no `calc_heal`.
    pub(crate) fn weigh_heal(
        batch: &mut ScriptBatch<'_>,
        heal: Heal,
    ) -> Option<Result<Num, CallError>> {
        if !Calls::defines(batch, Hook::CalcHeal) {
            return None;
        }
        let handle = HealHandle::new(heal, batch.ctx().view().clone());
        Some(Calls::amount(
            batch,
            Hook::CalcHeal,
            ScriptValue::of(handle),
        ))
    }

    /// Whether the mode's script defines `hook`, as its load found it.
    fn defines(batch: &ScriptBatch<'_>, hook: Hook) -> bool {
        batch.ctx().match_mode_spec().schema.hooks.contains(hook)
    }

    /// The number the mode's pure `hook` returns for `handle`, in `batch`.
    fn amount(
        batch: &mut ScriptBatch<'_>,
        hook: Hook,
        handle: ScriptValue,
    ) -> Result<Num, CallError> {
        let now = batch.world().resource::<SimTick>().start();
        let ctx = batch.ctx().clone();
        let script = ctx.match_mode_spec().schema.script;
        let value = batch.run(CallStart::pure(now), |batch| {
            batch.call_pure(HookCall {
                script,
                hook,
                args: (ctx, handle),
            })
        })?;
        let amount = match value.as_int() {
            Some(int) => Num::from_int(int),
            None => value.try_cast::<Num>(),
        };
        amount.ok_or_else(|| CallError::Api(ModeRefusal::NotAnAmount.refused()))
    }
}
