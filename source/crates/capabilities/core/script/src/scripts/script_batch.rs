use bevy_ecs::world::World;
use campfire_sim::StableId;

use crate::script_host::budget::Budget;
use crate::script_host::{ScriptHost, ScriptId};
use crate::scripts::call_start::{CallStart, Purity, Spent};
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::{Hook, HookSpec};
use crate::scripts::script_args::ScriptArgs;
use crate::scripts::script_budgets::ScriptBudgets;
use crate::scripts::script_failures::ScriptFailures;
use crate::scripts::script_value::ScriptValue;

/// A group of script calls in one stage: they share the view as the stage began, the `ctx` of
/// their calls, and the host, out of the world while they run.
#[derive(Debug)]
pub struct ScriptBatch<'a> {
    pub(super) world: &'a mut World,
    pub(super) host: &'a mut ScriptHost,
    pub(super) ctx: Ctx,
}

impl ScriptBatch<'_> {
    /// The `ctx` of the batch's calls.
    pub const fn ctx(&self) -> &Ctx {
        &self.ctx
    }

    /// Runs one call in the batch's frame: begins `start`, then runs `body`, which queues what
    /// the call does and calls its script; applies the frame in the call's tick when both succeed
    /// and the call writes. A failure applies nothing, for its caller to answer.
    pub fn run<T>(
        &mut self,
        start: CallStart,
        body: impl FnOnce(&mut ScriptBatch<'_>) -> Result<T, CallError>,
    ) -> Result<T, CallError> {
        let ctx = self.ctx.clone();
        // The frame's borrow ends before `body`, which borrows it again.
        ctx.frame().begin(self.world, start)?;
        let ran = body(self)?;
        if start.purity == Purity::Writes {
            ctx.apply(self.world);
        }
        Ok(ran)
    }

    /// Runs one call of `hook` as `run` does, and answers its end: a failure is recorded, for
    /// `id` if any, but a pool found spent by a call that waits, which its caller runs again in a
    /// later tick (02-engine-core.md, Operation pools).
    pub fn answer<T>(
        &mut self,
        start: CallStart,
        hook: Hook,
        id: Option<StableId>,
        body: impl FnOnce(&mut ScriptBatch<'_>) -> Result<T, CallError>,
    ) -> Answered<T> {
        match self.run(start, body) {
            Ok(ran) => Answered::Ran(ran),
            Err(CallError::PoolSpent) if start.when_spent() == Spent::Waits => Answered::Waits,
            Err(error) => {
                self.record(id, hook, &error);
                Answered::Failed
            }
        }
    }

    /// Calls the hook `H` of `script` with `args` in the running call, which writes, from the
    /// budget of its pool.
    pub fn call<H: HookSpec>(
        &mut self,
        script: ScriptId,
        args: H::Args,
    ) -> Result<ScriptValue, CallError> {
        let pool = self.ctx.frame().pool();
        let pool = pool.expect("a call that writes draws from a pool");
        let budgets = self.world.resource_mut::<ScriptBudgets>().into_inner();
        call_in::<H>(self.host, budgets.get_mut(pool), script, args)
    }

    /// Calls the hook `H` of `script` with `args` in the running call, which is pure, from no
    /// pool: only the limit per call bounds it.
    pub fn call_pure<H: HookSpec>(
        &mut self,
        script: ScriptId,
        args: H::Args,
    ) -> Result<ScriptValue, CallError> {
        debug_assert!(
            self.ctx.frame().pure(),
            "only a pure call draws from no pool"
        );
        call_in::<H>(self.host, &mut Budget::new(u64::MAX), script, args)
    }

    /// Records that a call of `hook`, for `id` if any, failed with `error`.
    pub fn record(&mut self, id: Option<StableId>, hook: Hook, error: &CallError) {
        self.world
            .resource_mut::<ScriptFailures>()
            .record(id, hook, error);
    }

    pub const fn world(&mut self) -> &mut World {
        self.world
    }
}

/// What became of a hook's call: it ran and gave `T`; it failed, and its failure is recorded; or
/// it found its pool spent and waits, unrecorded, for a later tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answered<T> {
    Ran(T),
    Failed,
    Waits,
}

/// Calls the hook `H` of `script` with `args` on `host`, drawing from `budget`.
fn call_in<H: HookSpec>(
    host: &mut ScriptHost,
    budget: &mut Budget,
    script: ScriptId,
    args: H::Args,
) -> Result<ScriptValue, CallError> {
    let hook = Hook::of::<H>();
    host.call(budget, script, hook.name, args.into_engine())
        .map(ScriptValue::of_engine)
        .map_err(CallError::from_script)
}
