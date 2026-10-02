use campfire_common::Ticks;
use campfire_script::ScriptId;
use campfire_sim::TickRate;

use crate::orders::Orders;
use crate::orders::ai_data::AiData;
use crate::orders::error::AiError;
use crate::scripts::hook::Hook;
use crate::scripts::script_book::ScriptBook;

/// A unit type's AI: its compiled script, and its period in whole ticks, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ai {
    pub(crate) script: ScriptId,
    pub(crate) period: Ticks,
}

impl Ai {
    /// The AI of `data`, whose script is `script`, which defines the hooks `scripts` gives, its
    /// period at `rate`.
    pub(crate) fn of(
        data: &AiData,
        script: ScriptId,
        scripts: &ScriptBook,
        rate: TickRate,
    ) -> Result<Ai, AiError> {
        let thinks = scripts
            .defines(Some(script), &[Hook::OnThink])
            .contains(Hook::OnThink);
        let period = Orders::ai_period(data, rate, thinks)?;
        Ok(Ai { script, period })
    }
}
