use bevy::ecs::component::Component;

/// A ring on the ground where a hit landed: it widens for `RING_SECONDS`, then goes.
#[derive(Component, Debug)]
pub(crate) struct Ring {
    /// When it appeared, in seconds of app time.
    pub(crate) since: f32,
}

/// How long a ring shows.
pub(crate) const RING_SECONDS: f32 = 0.35;

impl Ring {
    /// How wide the ring is `at` seconds of app time, as a multiple of its first width; `None`
    /// once it is done.
    pub(crate) fn scale(&self, at: f32) -> Option<f32> {
        let done = (at - self.since) / RING_SECONDS;
        (done < 1.0).then(|| 1.0 + 0.8 * done.max(0.0))
    }
}

#[cfg(test)]
mod tests;
