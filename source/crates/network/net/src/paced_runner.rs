use core::time::Duration;

use bevy_app::{App, AppExit, Plugin, PluginsState};
use bevy_time::{Fixed, Time, Virtual};
use campfire_net_endpoint::FrameWake;

/// Runs a headless app's frames, a server's, a local server's or a bot's, at its fixed ticks and as
/// its links have work: a frame, then a wait on its `FrameWake` until its next fixed tick is due,
/// so an idle app runs a frame a tick, and a packet that arrives between two ticks is read then
/// (02-engine-core.md, A headless app runs a frame at each tick and on each link's work).
#[derive(Debug)]
pub struct PacedRunner;

impl Plugin for PacedRunner {
    fn build(&self, app: &mut App) {
        app.set_runner(Self::run);
    }
}

impl PacedRunner {
    fn run(mut app: App) -> AppExit {
        let state = app.plugins_state();
        if state != PluginsState::Cleaned {
            debug_assert_eq!(
                state,
                PluginsState::Ready,
                "a headless app's plugins build at once"
            );
            app.finish();
            app.cleanup();
        }
        loop {
            app.update();
            if let Some(exit) = app.should_exit() {
                return exit;
            }
            let world = app.world();
            let wait = Self::wait(
                world.resource::<Time<Fixed>>(),
                world.resource::<Time<Virtual>>(),
            );
            world.resource::<FrameWake>().clone().wait(wait);
        }
    }

    /// The real time until the next fixed tick is due, the time a tick has left at the virtual
    /// time's speed, and at most one tick's length: while the virtual time stands, as a paused
    /// local match's does, the app still runs a frame a tick's length apart, which sees the pace
    /// change and the stop.
    fn wait(fixed: &Time<Fixed>, virtual_time: &Time<Virtual>) -> Duration {
        let period = fixed.timestep();
        let speed = f64::from(virtual_time.relative_speed());
        if virtual_time.is_paused() || speed <= 0.0 {
            return period;
        }
        let left = period.saturating_sub(fixed.overstep());
        left.div_f64(speed).min(period)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 30 ms tick of which 10 ms passed waits 20 ms at the normal speed, 10 ms at twice it, and
    /// 30 ms, one tick, at half of it and while paused.
    #[test]
    fn the_wait_is_the_time_to_the_next_tick_at_most_one_tick() {
        let tick = Duration::from_millis(30);
        let mut fixed = Time::<Fixed>::from_duration(tick);
        fixed.accumulate_overstep(Duration::from_millis(10));
        let wait = |speed: f32, paused: bool| {
            let mut virtual_time = Time::<Virtual>::default();
            virtual_time.set_relative_speed(speed);
            if paused {
                virtual_time.pause();
            }
            PacedRunner::wait(&fixed, &virtual_time)
        };
        assert_eq!(wait(1.0, false), Duration::from_millis(20));
        assert_eq!(wait(2.0, false), Duration::from_millis(10));
        assert_eq!(wait(0.5, false), tick);
        assert_eq!(wait(1.0, true), tick);
    }
}
