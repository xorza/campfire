use bevy_ecs::component::Component;
use campfire_math::Num;

/// A unit's stats, each the mode declares in the stat book's order: derived from its type and
/// level whenever they change, never state.
#[derive(Component, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct UnitStats {
    values: Vec<Num>,
}

impl UnitStats {
    pub(crate) fn values(&self) -> &[Num] {
        &self.values
    }

    /// The values to fill again, cleared.
    pub(crate) fn refill(&mut self) -> &mut Vec<Num> {
        self.values.clear();
        &mut self.values
    }
}
