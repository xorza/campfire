use bevy_ecs::world::World;
use campfire_common::Tick;
use campfire_script::rhai::{Dynamic, FuncArgs};
use campfire_script::{Budget, ScriptError, ScriptHost, ScriptId};
use campfire_sim::StableId;

use crate::scripts::call_start::CallStart;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::units::view::View;

/// A group of script calls in one stage: they share the view as the stage began, and the host,
/// out of the world while they run.
#[derive(Debug)]
pub(crate) struct ScriptBatch<'a> {
    world: &'a mut World,
    host: &'a mut ScriptHost,
}

impl ScriptBatch<'_> {
    /// Reads `view`, takes the host out of `world`, runs `calls`, puts the host back, and gives
    /// what `calls` gave.
    pub(crate) fn run<T>(
        world: &mut World,
        view: &View,
        calls: impl FnOnce(&mut ScriptBatch<'_>) -> T,
    ) -> T {
        view.read(world);
        let mut host = world
            .remove_non_send::<ScriptHost>()
            .expect("the core runs scripts");
        let given = calls(&mut ScriptBatch {
            world,
            host: &mut host,
        });
        world.insert_non_send(host);
        given
    }

    /// Calls `hook` of `script` with `args`, from the budget of `pool`.
    pub(crate) fn call(
        &mut self,
        pool: Pool,
        script: ScriptId,
        hook: Hook,
        args: impl FuncArgs,
    ) -> Result<Dynamic, ScriptError> {
        let budgets = self.world.resource_mut::<ScriptBudgets>().into_inner();
        self.host
            .call(budgets.get_mut(pool), script, hook.name(), args)
    }

    /// Calls `hook` of `script` with `args`, from the budget of `pool`, for its effects alone.
    pub(crate) fn call_hook(
        &mut self,
        pool: Pool,
        script: ScriptId,
        hook: Hook,
        args: impl FuncArgs,
    ) -> Result<(), CallError> {
        self.call(pool, script, hook, args)
            .map(drop)
            .map_err(CallError::from_script)
    }

    /// Runs one call of `hook` in `ctx`'s frame: begins `start`, then runs `body`, which queues
    /// what the call does and calls its script; applies the frame in tick `now` when both
    /// succeed, and otherwise records the failure, for `unit` if any, and applies nothing.
    pub(crate) fn hook_call(
        &mut self,
        ctx: &Ctx,
        now: Tick,
        start: CallStart,
        hook: Hook,
        unit: Option<StableId>,
        body: impl FnOnce(&mut ScriptBatch<'_>) -> Result<(), CallError>,
    ) {
        // The frame's borrow ends before `body`, which borrows it again.
        let begun = ctx.frame().begin(self.world, start);
        match begun.and_then(|()| body(self)) {
            Ok(()) => ctx.apply(self.world, now),
            Err(error) => self.record(unit, hook, error),
        }
    }

    /// Calls `hook` of `script` with `args` from no pool: only the limit per call bounds it.
    pub(crate) fn call_pure(
        &mut self,
        script: ScriptId,
        hook: Hook,
        args: impl FuncArgs,
    ) -> Result<Dynamic, ScriptError> {
        self.host
            .call(&mut Budget::new(u64::MAX), script, hook.name(), args)
    }

    /// Records that a call of `hook`, for `unit` if any, failed with `error`.
    pub(crate) fn record(&mut self, unit: Option<StableId>, hook: Hook, error: CallError) {
        self.world
            .non_send_mut::<ScriptFailures>()
            .record(unit, hook, error);
    }

    pub(crate) const fn world(&mut self) -> &mut World {
        self.world
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use bevy_ecs::world::World;

    use crate::units::view::View;

    /// Reads the units of `world` into its script view, as each batch of script calls does.
    pub fn read_view(world: &mut World) {
        let view = world.non_send::<View>().clone();
        view.read(world);
    }
}
