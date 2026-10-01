use bevy_ecs::world::World;
use campfire_math::Num;
use campfire_script::ScriptError;
use campfire_script::rhai::FuncArgs;
use campfire_sim::{SimTick, Tick};

use crate::combat::damage::Damage;
use crate::combat::damage_handle::DamageHandle;
use crate::mode::mode_book::ModeBook;
use crate::mode::mode_state::ModeState;
use crate::mode::picks::Picks;
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
        self.ctx
            .mode()
            .expect("mode calls run in a match with a mode")
    }

    /// Runs `hook` with `args` from `pool`: on success its state and choices commit and its
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
        let now = batch.world().resource::<SimTick>().start();
        let mut calls = Calls { batch, ctx, now };
        calls.begin(true);
        let script = calls.book().schema.script;
        let handle = DamageHandle::new(damage, ctx.view().clone());
        let returned = calls
            .batch
            .call_pure(script, Hook::CalcDamage, (ctx.clone(), handle));
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
        self.ctx.view().set_caller(0);
    }

    /// Commits the call's state and choices, then applies its effects in order.
    fn commit(&mut self) {
        let world = self.batch.world();
        {
            let frame = self.ctx.frame();
            world.resource_mut::<ModeState>().0.clone_from(&frame.state);
            world.resource_mut::<Picks>().clone_from(&frame.picks);
        }
        self.ctx.apply(world, self.now);
    }
}
