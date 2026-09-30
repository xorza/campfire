use bevy_ecs::resource::Resource;
use campfire_script::Budget;

use crate::units::script_limits::ScriptLimits;

/// What each pool has left in the running tick. Not state: every tick starts with the full
/// limits, and a call that ran out changed nothing.
#[derive(Resource, Debug)]
pub(crate) struct ScriptBudgets {
    limits: ScriptLimits,
    pub(crate) input: Budget,
    pub(crate) think: Budget,
    pub(crate) mode: Budget,
}

impl ScriptBudgets {
    pub(crate) const fn new(limits: ScriptLimits) -> ScriptBudgets {
        ScriptBudgets {
            limits,
            input: Budget::new(limits.input),
            think: Budget::new(limits.think),
            mode: Budget::new(limits.mode),
        }
    }

    pub(crate) const fn begin_tick(&mut self) {
        *self = ScriptBudgets::new(self.limits);
    }
}
