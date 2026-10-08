use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_math::Num;
use campfire_script::ScriptError;
use campfire_script::rhai::{Dynamic, FuncArgs};
use campfire_sim::SimTick;

use crate::combat::damage::Damage;
use crate::combat::damage_handle::DamageHandle;
use crate::combat::heal::Heal;
use crate::combat::heal_handle::HealHandle;
use crate::mode::choices::Choices;
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_call::ModeCall;
use crate::mode::mode_state::ModeState;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::{ApiError, CallError};
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;

/// The mode calls of one batch, at one time.
#[derive(Debug)]
pub(crate) struct Calls<'a, 'w> {
    pub(crate) batch: &'a mut ScriptBatch<'w>,
    pub(crate) ctx: &'a Ctx,
    now: Tick,
}

impl Calls<'_, '_> {
    /// Runs `calls` in one script batch at the time `now`, and gives what `calls` gave.
    pub(crate) fn batch<T>(
        world: &mut World,
        ctx: &Ctx,
        now: Tick,
        calls: impl FnOnce(&mut Calls<'_, '_>) -> T,
    ) -> T {
        ScriptBatch::run(world, ctx.view(), |batch| {
            calls(&mut Calls { batch, ctx, now })
        })
    }

    /// The match's mode.
    pub(crate) fn book(&self) -> &ModeBook {
        ModeBook::of(self.ctx).expect("mode calls run in a match with a mode")
    }

    /// Runs `hook` with `args` from `pool`: on success its state, choices and ids commit and its
    /// effects apply; on failure nothing changes.
    pub(crate) fn run(
        &mut self,
        pool: Pool,
        hook: Hook,
        args: impl FuncArgs,
    ) -> Result<(), ScriptError> {
        self.begin(false);
        let script = self.book().schema.script;
        drop(self.batch.call(pool, script, hook, args)?);
        self.commit();
        Ok(())
    }

    /// `calc_damage` of `damage` in `batch`, its `ctx` pure: the number it returns, an integer
    /// as a number.
    pub(crate) fn weigh(
        batch: &mut ScriptBatch<'_>,
        ctx: &Ctx,
        damage: Damage,
    ) -> Result<Num, CallError> {
        let handle = DamageHandle::new(damage, ctx.view().clone());
        Calls::amount(batch, ctx, Hook::CalcDamage, Dynamic::from(handle))
    }

    /// The amount of `heal` before the heal scale: what the mode's `calc_heal` returns for it,
    /// a pure call in `batch`; an error when the call fails or returns no number.
    pub(crate) fn weigh_heal(
        batch: &mut ScriptBatch<'_>,
        ctx: &Ctx,
        heal: Heal,
    ) -> Result<Num, CallError> {
        let handle = HealHandle::new(heal, ctx.view().clone());
        Calls::amount(batch, ctx, Hook::CalcHeal, Dynamic::from(handle))
    }

    /// The number the mode's pure `hook` returns for `handle`, in `batch`.
    fn amount(
        batch: &mut ScriptBatch<'_>,
        ctx: &Ctx,
        hook: Hook,
        handle: Dynamic,
    ) -> Result<Num, CallError> {
        let now = batch.world().resource::<SimTick>().start();
        let mut calls = Calls { batch, ctx, now };
        calls.begin(true);
        let script = calls.book().schema.script;
        let returned = calls.batch.call_pure(script, hook, (ctx.clone(), handle));
        let value = returned.map_err(CallError::from_script)?;
        let amount = match value.as_int() {
            Ok(int) => Num::from_int(int),
            Err(_) => value.try_cast::<Num>(),
        };
        amount.ok_or(CallError::Api(ApiError::NotAnAmount))
    }

    /// Starts a mode call on the mode's state as it stands; `pure` for a hook whose `ctx` only
    /// reads.
    fn begin(&mut self, pure: bool) {
        let world = self.batch.world();
        self.ctx.frame().begin_mode(world, pure);
    }

    /// Commits the call's state and choices, then applies its effects in order.
    fn commit(&mut self) {
        let world = self.batch.world();
        {
            let frame = self.ctx.frame();
            let call = ModeCall::of(&frame);
            // A write marks the state changed, so a call that changed none writes nothing.
            if world.resource::<ModeState>().0 != call.state {
                world.resource_mut::<ModeState>().0.clone_from(&call.state);
            }
            if *world.resource::<Choices>() != call.choices {
                world.resource_mut::<Choices>().clone_from(&call.choices);
            }
        }
        self.ctx.apply(world, self.now);
    }
}
