use crate::interpolation_sample_cx::InterpolationSampleCx;
use core::fmt;

/// Makes the visual value `V` of a fixed-tick component `C` from `C`'s values at the last two
/// fixed ticks and the frame's place between them.
pub type FrameMap<C, V> = fn(previous: &C, current: &C, cx: InterpolationSampleCx) -> V;

/// What a frame shows of a fixed-tick component `C`: its visual component `V`, which `map` makes
/// from `C`'s last two fixed ticks, while `C` keeps the simulated value (11-net-stack/replication.md,
/// The value on screen is its own component). Of the rules of `C` that match an archetype, the one
/// of the highest priority writes, the first registered among equals.
pub struct FrameRule<C, V> {
    pub(crate) map: Option<FrameMap<C, V>>,
    pub(crate) priority: usize,
}

impl<C, V> FrameRule<C, V> {
    /// Writes `V` by `map`, at priority 0.
    pub const fn new(map: FrameMap<C, V>) -> Self {
        Self {
            map: Some(map),
            priority: 0,
        }
    }

    /// Writes nothing on the archetypes it wins: a filtered rule of a higher priority that keeps
    /// matching entities out of a broader one.
    pub const fn disabled() -> Self {
        Self {
            map: None,
            priority: 0,
        }
    }

    /// The priority among the rules of `C` that match an archetype: the highest wins.
    #[must_use]
    pub const fn priority(mut self, priority: usize) -> Self {
        self.priority = priority;
        self
    }
}

impl<C, V> Clone for FrameRule<C, V> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<C, V> Copy for FrameRule<C, V> {}

impl<C, V> fmt::Debug for FrameRule<C, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameRule")
            .field("writes", &self.map.is_some())
            .field("priority", &self.priority)
            .finish()
    }
}
