use bevy_ecs::resource::Resource;
use campfire_script::ScriptId;

use crate::units::unit_type::UnitType;

/// The AI of each unit type that has one: package data, not state. A restore loads it from the
/// packages, as a new match does.
#[derive(Resource, Debug, Default)]
pub(crate) struct AiBook {
    by_type: Vec<Option<Ai>>,
}

/// A unit type's AI: its compiled script, and its period in whole ticks, at least one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ai {
    pub(crate) script: ScriptId,
    pub(crate) period: u32,
}

impl AiBook {
    pub(crate) fn get(&self, unit_type: UnitType) -> Option<Ai> {
        self.by_type.get(unit_type.index()).copied().flatten()
    }

    /// Gives `unit_type` its AI; a type has one at most.
    pub(crate) fn set(&mut self, unit_type: UnitType, ai: Ai) {
        let index = unit_type.index();
        if self.by_type.len() <= index {
            self.by_type.resize(index + 1, None);
        }
        assert!(self.by_type[index].is_none(), "a unit type has one AI");
        self.by_type[index] = Some(ai);
    }
}
