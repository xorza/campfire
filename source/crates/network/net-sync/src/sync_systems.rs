use bevy_ecs::schedule::SystemSet;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum SyncSystems {
    /// Synchronize local clocks and cursors to remote timelines using RTT and jitter information.
    Sync,
}
