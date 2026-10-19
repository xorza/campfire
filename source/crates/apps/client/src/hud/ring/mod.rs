use std::time::Duration;

use bevy::ecs::component::Component;

/// A ring on the ground where a hit landed, as wide as its unit's reach at first: it widens for
/// `RING_TIME`, then goes.
#[derive(Component, Debug)]
pub(crate) struct Ring {
    /// When it appeared, in app time.
    pub(crate) since: Duration,
    /// Its unit's reach, its first scale.
    pub(crate) reach: f32,
}

/// How long a ring shows.
const RING_TIME: Duration = Duration::from_millis(350);

impl Ring {
    /// How wide the ring is at `now` of app time, its scale: its reach times its growth; `None`
    /// once it is done. Only the difference of two times becomes an `f32`, so a long session loses
    /// no precision.
    pub(crate) fn scale(&self, now: Duration) -> Option<f32> {
        let done = now.saturating_sub(self.since).as_secs_f32() / RING_TIME.as_secs_f32();
        (done < 1.0).then_some(self.reach * (1.0 + 0.8 * done))
    }
}

#[cfg(test)]
mod tests;
