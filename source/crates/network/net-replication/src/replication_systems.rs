use bevy_ecs::prelude::SystemSet;

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum ReplicationSystems {
    /// In `PreUpdate`: applies the replication messages a client received to its world.
    Receive,
    /// In `PostUpdate`: the server's replication send, which buffers its messages in each
    /// link's transport.
    Send,
    /// In `PostUpdate`: the cut of each confirmed history by its timeline's retention, which the
    /// diff receivers' prune follows to the history's front.
    CutHistory,
}
