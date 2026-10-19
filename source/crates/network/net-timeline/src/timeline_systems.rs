use bevy_ecs::schedule::SystemSet;

/// Shared ordering for systems that advance network timelines.
#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone, Copy)]
pub enum TimelineSystems {
    /// Drives the internal state of network timelines forward in `PreUpdate`.
    Advance,
    /// Increments the application-global [`LocalTimeline`](crate::local_timeline::LocalTimeline) in
    /// `FixedFirst`.
    IncrementLocal,
}
