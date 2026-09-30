use campfire_script::ScriptId;
use campfire_sim::Ticks;

/// A unit type's AI: its compiled script, and its period in whole ticks, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ai {
    pub(crate) script: ScriptId,
    pub(crate) period: Ticks,
}
