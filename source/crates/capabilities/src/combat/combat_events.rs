use std::fmt;

use crate::combat::combat_event::CombatEvent;
use crate::scripts::script_batch::ScriptBatch;

/// What answers each combat event: the hooks of the modifiers that hear it, which
/// `ModifierHooks` runs with the `ctx` ability scripts get. Package data, not state.
pub(crate) struct CombatEvents(Box<HearFn>);

type HearFn = dyn Fn(&mut ScriptBatch<'_>, CombatEvent);

impl CombatEvents {
    pub(crate) fn new(hear: impl Fn(&mut ScriptBatch<'_>, CombatEvent) + 'static) -> CombatEvents {
        CombatEvents(Box::new(hear))
    }

    /// Answers `event` in `batch`.
    pub(crate) fn hear(&self, batch: &mut ScriptBatch<'_>, event: CombatEvent) {
        (self.0)(batch, event);
    }
}

impl fmt::Debug for CombatEvents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CombatEvents")
    }
}
