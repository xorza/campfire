/// The settings of a `Timeline`, the one part of it an app writes.
pub trait TimelineSetup: Send + Sync + Sized + 'static {
    /// The state the timeline keeps beside its instant; used by the timeline's internals.
    type State;
}
