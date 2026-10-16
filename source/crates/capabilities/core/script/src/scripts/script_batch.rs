use bevy_ecs::world::World;
use campfire_sim::StableId;

use crate::script_host::budget::Budget;
use crate::script_host::{ScriptHost, ScriptId};
use crate::scripts::call_start::{CallStart, Purity};
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
pub struct ScriptBatch<'a> {
    pub(super) world: &'a mut World,
    pub(super) host: &'a mut ScriptHost,
    pub(super) ctx: Ctx,
}

/// A call of `hook` of `script` with `args`, one argument for each of the hook's parameters.
#[derive(Debug, Clone)]
pub struct HookCall<A> {
    pub script: ScriptId,
    pub hook: Hook,
    pub args: A,
}

impl ScriptBatch<'_> {
    /// The `ctx` of the batch's calls.
    pub const fn ctx(&self) -> &Ctx {
        &self.ctx
    }

    /// Runs one call in the batch's frame: begins `start`, then runs `body`, which queues what
    /// the call does and calls its script; applies the frame in the call's tick when both succeed
    /// and the call writes. A failure applies nothing, and its caller records it, or, for
    /// `CallError::PoolSpent`, may wait for a later tick.
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

    /// Runs one call of `hook` as `run` does, and records its failure, for `id` if any: an
    /// action's, a delivery's or a modifier's hook, which fails when its pool is spent.
    pub fn hook_call(
        &mut self,
        start: CallStart,
        hook: Hook,
        id: Option<StableId>,
        body: impl FnOnce(&mut ScriptBatch<'_>) -> Result<(), CallError>,
    ) {
        if let Err(error) = self.run(start, body) {
            self.record(id, hook, error);
        }
    }

    /// Makes `call` in the running call, which writes, from the budget of `pool`.
    pub fn call(
        &mut self,
        pool: Pool,
        call: HookCall<impl ScriptArgs>,
    ) -> Result<ScriptValue, CallError> {
        debug_assert!(!self.ctx.frame().pure(), "a pure call draws from no pool");
        let budgets = self.world.resource_mut::<ScriptBudgets>().into_inner();
        call_in(self.host, budgets.get_mut(pool), call)
    }

    /// Makes `call` in the running call, which is pure, from no pool: only the limit per call
    /// bounds it.
    pub fn call_pure(&mut self, call: HookCall<impl ScriptArgs>) -> Result<ScriptValue, CallError> {
        debug_assert!(
            self.ctx.frame().pure(),
            "only a pure call draws from no pool"
        );
        call_in(self.host, &mut Budget::new(u64::MAX), call)
    }

    /// Records that a call of `hook`, for `id` if any, failed with `error`.
    pub fn record(&mut self, id: Option<StableId>, hook: Hook, error: CallError) {
        self.world
            .non_send_mut::<ScriptFailures>()
            .record(id, hook, error);
    }

    pub const fn world(&mut self) -> &mut World {
        self.world
    }
}

/// Makes `call` on `host`, drawing from `budget`.
fn call_in<A: ScriptArgs>(
    host: &mut ScriptHost,
    budget: &mut Budget,
    call: HookCall<A>,
) -> Result<ScriptValue, CallError> {
    let HookCall { script, hook, args } = call;
    debug_assert_eq!(
        A::LEN,
        hook.params(),
        "{hook:?} takes one argument a parameter"
    );
    host.call(budget, script, hook.name(), args.into_engine())
        .map(ScriptValue::of_engine)
        .map_err(CallError::from_script)
}
