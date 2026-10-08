use std::time::Duration;

use bevy::ecs::component::Component;

/// A ring on the ground where a hit landed: it widens for `RING_TIME`, then goes.
#[derive(Component, Debug)]
pub(crate) struct Ring {
    /// When it appeared, in app time.
    pub(crate) since: Duration,
}

/// How long a ring shows.
const RING_TIME: Duration = Duration::from_millis(350);

impl Ring {
    /// How wide the ring is at `now` of app time, as a multiple of its first width; `None` once
    /// it is done. Only the difference of two times becomes an `f32`, so a long session loses no
    /// precision.
    pub(crate) fn scale(&self, now: Duration) -> Option<f32> {
        let done = now.saturating_sub(self.since).as_secs_f32() / RING_TIME.as_secs_f32();
        (done < 1.0).then_some(1.0 + 0.8 * done)
    }
}

#[cfg(test)]
mod tests;
