use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;

/// The units that carry a live stat change, sorted: their stats refresh every pass, as their
/// sources' stats change without their own state changing. Each refresh rebuilds it from the
/// units it visits, which hold every unit on it, so a unit gone drops off. Derived, never state.
#[derive(Resource, Debug, Default)]
pub(crate) struct LiveCarriers(Vec<Entity>);

impl LiveCarriers {
    pub(crate) fn units(&self) -> &[Entity] {
        &self.0
    }

    /// Starts the list again, empty, for a refresh to fill in order.
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }

    /// Adds `unit`, which comes after every unit on the list.
    pub(crate) fn push(&mut self, unit: Entity) {
        debug_assert!(self.0.last().is_none_or(|&last| last < unit));
        self.0.push(unit);
    }
}
