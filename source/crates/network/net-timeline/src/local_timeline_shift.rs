use bevy_ecs::event::Event;

/// Announces a whole-tick discontinuity in the application-global
/// [`LocalTimeline`](crate::local_timeline::LocalTimeline).
///
/// Input buffers, prediction histories, and other data indexed by local simulation ticks must
/// shift by the same delta. Remote and interpolation histories already use their own time domains
/// and must not observe this event.
#[derive(Event, Debug, Clone, Copy)]
pub struct LocalTimelineShift {
    /// Whole ticks added to the local simulation clock.
    pub delta: i32,
}
