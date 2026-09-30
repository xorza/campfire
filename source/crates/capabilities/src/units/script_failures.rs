use campfire_sim::StableId;

use crate::units::error::CallError;

/// The script calls of the running tick that failed. A failed call changed nothing; the list is
/// for the host to see. Not state: it empties each tick. A non-send resource, as a script's
/// raised value is not `Send`.
#[derive(Debug, Default)]
pub struct ScriptFailures(pub(crate) Vec<ScriptFailure>);

/// A call that failed: the unit it ran for, its hook, and why.
#[derive(Debug, Clone)]
pub struct ScriptFailure {
    pub unit: StableId,
    pub hook: Hook,
    pub error: CallError,
}

/// A hook the engine calls in a script.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hook {
    /// `on_cast(ctx, caster, target)`, as an ability's cast resolves.
    OnCast,
    /// `think(ctx, unit)`, as an AI unit thinks.
    Think,
}

impl ScriptFailures {
    pub fn get(&self) -> &[ScriptFailure] {
        &self.0
    }
}

impl Hook {
    /// The script function the engine calls.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Hook::OnCast => "on_cast",
            Hook::Think => "think",
        }
    }

    /// How many parameters the function takes.
    pub(crate) const fn params(self) -> usize {
        match self {
            Hook::OnCast => 3,
            Hook::Think => 2,
        }
    }
}
