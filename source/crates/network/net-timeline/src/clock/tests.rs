use bevy_app::App;
use bevy_time::TimePlugin;
use campfire_math::TickRate;

use super::*;
use crate::timeline_plugins::TimelinePlugins;

fn app(source: ClockSource, strategy: TimeUpdateStrategy) -> App {
    let mut app = App::new();
    app.insert_resource(Clock::new(source));
    app.add_plugins((
        TimePlugin,
        TimelinePlugins {
            tick_rate: TickRate::try_from(100).unwrap(),
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
    assert_eq!(clock.reads, TimeSource::Os);
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
