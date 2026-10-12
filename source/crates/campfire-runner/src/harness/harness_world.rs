use bevy_ecs::world::World;
use campfire_sim::{EntityIndex, SimWorld, SnapshotError};

/// What the harness gives a match for the tests and benches of every crate.
pub trait HarnessWorldInternals {
    /// Restores `snapshot` into a restore target, in place of what the last restore left.
    fn restore_in_place(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError>;
}

impl HarnessWorldInternals for World {
    fn restore_in_place(&mut self, snapshot: &[u8]) -> Result<(), SnapshotError> {
        let units: Vec<_> = self
            .resource::<EntityIndex>()
            .iter()
            .map(|(_, entity)| entity)
            .collect();
        for entity in units {
            self.despawn(entity);
        }
        self.restore(snapshot)
    }
}
