use bevy_ecs::world::World;
use campfire_script::{Budget, ScriptError, ScriptHost, ScriptId};
use campfire_sim::StableId;

use crate::scripts::call_start::CallStart;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_args::ScriptArgs;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::scripts::script_value::ScriptValue;

/// A group of script calls in one stage: they share the view as the stage began, the `ctx` of
/// their calls, and the host, out of the world while they run.
#[derive(Debug)]
pub(crate) struct ScriptBatch<'a> {
    pub(super) world: &'a mut World,
    pub(super) host: &'a mut ScriptHost,
    pub(super) ctx: Ctx,
}

impl ScriptBatch<'_> {
    /// The `ctx` of the batch's calls.
    pub(crate) const fn ctx(&self) -> &Ctx {
        &self.ctx
    }

    /// Calls `hook` of `script` with `args`, from the budget of `pool`.
    pub(crate) fn call(
        &mut self,
        pool: Pool,
        script: ScriptId,
        hook: Hook,
        args: impl ScriptArgs,
    ) -> Result<ScriptValue, ScriptError> {
        let budgets = self.world.resource_mut::<ScriptBudgets>().into_inner();
        self.host
            .call(
                budgets.get_mut(pool),
                script,
                hook.name(),
                args.into_engine(),
            )
            .map(ScriptValue::of_engine)
    }

    /// Calls `hook` of `script` with `args`, from the budget of `pool`, for its effects alone.
    pub(crate) fn call_hook(
        &mut self,
        pool: Pool,
        script: ScriptId,
        hook: Hook,
        args: impl ScriptArgs,
    ) -> Result<(), CallError> {
        self.call(pool, script, hook, args)
            .map(drop)
            .map_err(CallError::from_script)
    }

    /// Runs one call of `hook` in the batch's frame: begins `start`, then runs `body`, which queues
    /// what the call does and calls its script; applies the frame in the call's tick when both
    /// succeed, and otherwise records the failure, for `id` if any, and applies nothing.
    pub(crate) fn hook_call(
        &mut self,
        start: CallStart,
        hook: Hook,
        id: Option<StableId>,
        body: impl FnOnce(&mut ScriptBatch<'_>) -> Result<(), CallError>,
    ) {
        let ctx = self.ctx.clone();
        // The frame's borrow ends before `body`, which borrows it again.
        let begun = ctx.frame().begin(self.world, start);
        match begun.and_then(|()| body(self)) {
            Ok(()) => ctx.apply(self.world),
            Err(error) => self.record(id, hook, error),
        }
    }

    /// Calls `hook` of `script` with `args` from no pool: only the limit per call bounds it.
    pub(crate) fn call_pure(
        &mut self,
        script: ScriptId,
        hook: Hook,
        args: impl ScriptArgs,
    ) -> Result<ScriptValue, ScriptError> {
        self.host
            .call(
                &mut Budget::new(u64::MAX),
                script,
                hook.name(),
                args.into_engine(),
            )
            .map(ScriptValue::of_engine)
    }

    /// Records that a call of `hook`, for `id` if any, failed with `error`.
    pub(crate) fn record(&mut self, id: Option<StableId>, hook: Hook, error: CallError) {
        self.world
            .non_send_mut::<ScriptFailures>()
            .record(id, hook, error);
    }

    pub(crate) const fn world(&mut self) -> &mut World {
        self.world
    }
}
