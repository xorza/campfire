//! The clock every networking read takes.

use bevy_ecs::prelude::*;
use bevy_platform::time::Instant as OsInstant;
use bevy_time::{Real, Time, TimeUpdateStrategy};
use core::time::Duration;

use crate::clock_source::ClockSource;
use campfire_net_core::Instant;

/// An app's network clock: ping stamps, packet send and receive times, retransmit timers and the
/// bandwidth bucket all read it (11-net-stack/time.md, One clock per app, owned by the stack).
///
/// `ClockPlugin` inserts one on [`ClockSource::Auto`] unless
/// the app holds one already, and refreshes it at the start of each frame.
#[derive(Resource, Debug)]
pub struct Clock {
    source: ClockSource,
    /// What the source resolved to on the last frame.
    reads: Reading,
    /// The OS moment of the app's first update, the origin of every [`Instant`].
    first_update: Option<OsInstant>,
    /// The start of the current frame.
    frame: Instant,
    /// The time since the previous frame's start.
    delta: Duration,
}

/// Where a [`Clock`] reads, once [`ClockSource::Auto`] is resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    Os,
    Frame,
}

impl Reading {
    /// `Auto` reads the OS until a frame shows the app's time is driven.
    const fn of(source: ClockSource, driven: bool) -> Self {
        match source {
            ClockSource::Os => Self::Os,
            ClockSource::Frame => Self::Frame,
            ClockSource::Auto if driven => Self::Frame,
            ClockSource::Auto => Self::Os,
        }
    }
}

impl Default for Clock {
    fn default() -> Self {
        Self::new(ClockSource::default())
    }
}

impl Clock {
    pub const fn new(source: ClockSource) -> Self {
        Self {
            source,
            reads: Reading::of(source, false),
            first_update: None,
            frame: Instant::ZERO,
            delta: Duration::ZERO,
        }
    }

    pub const fn source(&self) -> ClockSource {
        self.source
    }

    /// The current moment: the OS time since the first update on [`ClockSource::Os`], the frame's
    /// start on [`ClockSource::Frame`], and [`Instant::ZERO`] before the first update.
    pub fn now(&self) -> Instant {
        match (self.reads, self.first_update) {
            (Reading::Os, Some(first_update)) => Instant::from_elapsed(first_update.elapsed()),
            (Reading::Frame, _) | (Reading::Os, None) => self.frame,
        }
    }

    /// The OS moment every [`Instant`] counts from, when the clock reads the OS and the app ran
    /// its first update: a thread off the frame reads the same time by it.
    pub const fn os_origin(&self) -> Option<OsInstant> {
        match self.reads {
            Reading::Os => self.first_update,
            Reading::Frame => None,
        }
    }

    /// The start of the current frame, the same on both sources.
    pub const fn frame(&self) -> Instant {
        self.frame
    }

    /// The time from the previous frame's start to this one's.
    pub const fn delta(&self) -> Duration {
        self.delta
    }

    pub(crate) fn update(
        mut clock: ResMut<'_, Self>,
        time: Res<'_, Time<Real>>,
        strategy: Option<Res<'_, TimeUpdateStrategy>>,
    ) {
        let driven = !matches!(
            strategy.as_deref(),
            None | Some(TimeUpdateStrategy::Automatic)
        );
        clock.reads = Reading::of(clock.source, driven);
        clock.first_update = time.first_update();
        clock.frame = Instant::from_elapsed(time.elapsed());
        clock.delta = time.delta();
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use core::time::Duration;

    use crate::clock::{Clock, Reading};
    use crate::clock_source::ClockSource;
    use campfire_net_core::Instant;

    impl Clock {
        /// A [`ClockSource::Frame`] clock whose current frame starts at `frame`, `delta` after the
        /// previous one, as a test hands it to code that reads a clock.
        pub const fn frame_at(frame: Instant, delta: Duration) -> Self {
            Self {
                source: ClockSource::Frame,
                reads: Reading::of(ClockSource::Frame, false),
                first_update: None,
                frame,
                delta,
            }
        }
    }
}

#[cfg(test)]
mod tests;
