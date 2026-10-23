/// Context passed to interpolation functions that need more than a normalized fraction.
///
/// Most interpolation functions only need [`Self::t`] and can use the [`LerpFn`](crate::LerpFn)
/// shape. More advanced functions, such as Hermite interpolation with endpoint velocities, can use
/// [`Self::sample_delta_secs`] to scale velocities by the actual sample interval.
///
/// Delayed interpolation computes [`Self::sample_delta_secs`] from the bracketing history ticks and
/// the configured tick rate. Frame interpolation and visual correction populate it from the fixed
/// timestep.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InterpolationSampleCx {
    /// Normalized interpolation fraction in `[0.0, 1.0]`.
    pub t: f32,
    /// Duration between the bracketing samples, in seconds, when available.
    pub sample_delta_secs: Option<f32>,
}

impl InterpolationSampleCx {
    /// Creates a context from a normalized interpolation fraction and an optional sample
    /// interval.
    pub const fn new(t: f32, sample_delta_secs: Option<f32>) -> Self {
        Self {
            t,
            sample_delta_secs,
        }
    }
}
