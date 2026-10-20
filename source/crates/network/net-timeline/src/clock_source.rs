use bevy_reflect::Reflect;

/// Where a `Clock` reads the time (11-net-stack/time.md, An external driver drives Bevy's time, and
/// the `Frame` source follows it) (11-net-stack/time.md, The default source follows how the app's
/// time runs).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Reflect)]
pub enum ClockSource {
    /// [`Os`](Self::Os) while the app's `TimeUpdateStrategy` is `Automatic`, and
    /// [`Frame`](Self::Frame) once a harness drives it: an app whose time is driven never reads
    /// the OS clock for its network while its ticks follow the driven time.
    #[default]
    Auto,
    /// The OS monotonic clock at each read, so a ping's stamps are the moments it is sent and
    /// received. A real-socket IO needs it.
    Os,
    /// The app's `Time<Real>` of the current frame, which follows a driven
    /// `TimeUpdateStrategy`: a harness drives the app's time the one way Bevy defines, and the
    /// network clock follows it.
    Frame,
}
