use bevy_ecs::world::World;
use campfire_script::rhai::{Dynamic, FuncArgs};
use campfire_script::{ScriptError, ScriptHost, ScriptId};
use campfire_sim::StableId;

use crate::units::error::CallError;
use crate::units::hook::Hook;
use crate::units::pool::Pool;
use crate::units::script_budgets::ScriptBudgets;
use crate::units::script_failures::ScriptFailures;
use crate::units::script_view::View;

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
