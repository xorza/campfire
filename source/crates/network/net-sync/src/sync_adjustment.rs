/// What a timeline synchronization sample does to the controlled clock.
#[derive(Debug)]
pub enum SyncAdjustment {
    /// Snap the clock to its objective.
    Resync,
    /// Run at this speed relative to the system clock.
    SpeedAdjust(f32),
    /// Run at the nominal speed, exactly 1.0: the error is inside the band, or not yet a run.
    Nominal,
}
