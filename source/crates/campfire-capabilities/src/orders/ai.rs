use campfire_common::Ticks;
use campfire_script::ScriptId;
use campfire_sim::TickRate;

use crate::orders::ai_data::AiData;
use crate::orders::error::AiError;
use crate::scripts::hook::Hook;
use crate::scripts::script_book::ScriptBook;
use crate::scripts::script_role::ScriptRole;

/// A unit type's AI: its compiled script, and its period in whole ticks, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ai {
    pub(crate) script: ScriptId,
    pub(crate) period: Ticks,
}

impl Ai {
    /// The AI of `data`, whose script is `script`, which defines the hooks `scripts` gives, its
    /// period at `rate`, a tick at the least; an error for a period too long to count, or a
    /// script that defines no `on_think`.
    pub(crate) fn of(
        data: &AiData,
        script: ScriptId,
        scripts: &ScriptBook,
        rate: TickRate,
    ) -> Result<Ai, AiError> {
        let thinks = scripts
            .defines(Some(script), ScriptRole::Ai)
            .contains(Hook::OnThink);
        let period = rate.duration(data.think_ms).ok_or(AiError::TimeTooLarge)?;
        if !thinks {
            return Err(AiError::NoThink);
        }
        Ok(Ai { script, period })
    }
}
