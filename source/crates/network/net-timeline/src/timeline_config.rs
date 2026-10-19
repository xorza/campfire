use bevy_ecs::component::Component;

/// Configuration for a `Timeline`.
///
/// The user should only modify the configuration.
pub trait TimelineConfig: Component + Send + Sync + Sized + 'static {
    /// The state the timeline keeps beside its instant; used by the timeline's internals.
    type State;
}
