//! The clock every networking read takes.

use bevy_ecs::prelude::*;
use bevy_platform::time::Instant as OsInstant;
use bevy_time::{Real, Time, TimeUpdateStrategy};
use core::time::Duration;

use crate::clock_source::ClockSource;
use campfire_net_core::Instant;

/// An app's network clock: ping stamps, packet send and receive times, retransmit timers and the
/// bandwidth bucket all read it.
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
mod tests {
    use super::*;
    use bevy_app::App;
    use bevy_time::TimePlugin;

    use crate::timeline_plugin::TimelinePlugin;
    use campfire_net_core::TickRate;
    use core::num::NonZeroU16;

    fn app(source: ClockSource, strategy: TimeUpdateStrategy) -> App {
        let mut app = App::new();
        app.insert_resource(Clock::new(source));
        app.add_plugins((
            TimePlugin,
            TimelinePlugin {
                tick_rate: TickRate::new(NonZeroU16::new(100).unwrap()),
            },
        ));
        app.insert_resource(strategy);
        app
    }

    #[test]
    fn frame_clock_follows_the_driven_time() {
        let mut app = app(
            ClockSource::Frame,
            TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)),
        );
        assert_eq!(app.world().resource::<Clock>().now(), Instant::ZERO);
        // The first update sets Bevy's origin and advances nothing; each later one advances 16 ms.
        app.update();
        app.update();
        app.update();
        let clock = app.world().resource::<Clock>();
        assert_eq!(
            clock.now(),
            Instant::from_elapsed(Duration::from_millis(32))
        );
        assert_eq!(clock.frame(), clock.now());
        assert_eq!(clock.delta(), Duration::from_millis(16));
    }

    #[test]
    fn auto_follows_a_driven_time_and_reads_the_os_otherwise() {
        // Driven: the same 16 ms steps as a `Frame` clock, 32 ms after three updates.
        let mut driven = app(
            ClockSource::Auto,
            TimeUpdateStrategy::ManualDuration(Duration::from_millis(16)),
        );
        driven.update();
        driven.update();
        driven.update();
        assert_eq!(
            driven.world().resource::<Clock>().now(),
            Instant::from_elapsed(Duration::from_millis(32))
        );
        // A driven clock has no OS origin for a thread to read by.
        assert_eq!(driven.world().resource::<Clock>().os_origin(), None);
        // Real time: the OS read comes at or after the frame's start, and a thread that reads the
        // OS time since the origin reads no earlier.
        let mut real = app(ClockSource::Auto, TimeUpdateStrategy::Automatic);
        real.update();
        real.update();
        let clock = real.world().resource::<Clock>();
        assert!(clock.now() >= clock.frame());
        assert_eq!(clock.source(), ClockSource::Auto);
        let origin = clock.os_origin().unwrap();
        let before = clock.now();
        assert!(Instant::from_elapsed(origin.elapsed()) >= before);
    }

    /// `Default` is `new` of the default source, `Auto`, which reads the OS before any frame.
    #[test]
    fn default_is_the_auto_clock() {
        let clock = Clock::default();
        assert_eq!(clock.source(), ClockSource::Auto);
        assert_eq!(clock.reads, Reading::Os);
        assert_eq!(Clock::new(ClockSource::Auto).reads, clock.reads);
    }

    #[test]
    fn os_clock_reads_at_or_after_the_frame_start() {
        // On real time the two sources share the first update as their origin, and the OS read
        // comes after the frame's start: the contract is the order, as the OS time is not exact.
        let mut app = app(ClockSource::Os, TimeUpdateStrategy::Automatic);
        assert_eq!(app.world().resource::<Clock>().now(), Instant::ZERO);
        app.update();
        app.update();
        let clock = app.world().resource::<Clock>();
        assert!(clock.now() >= clock.frame());
        assert_eq!(clock.source(), ClockSource::Os);
    }
}
