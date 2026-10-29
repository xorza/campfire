use core::time::Duration;

use bevy_ecs::prelude::*;
use bevy_time::{Fixed, Time};
use campfire_net_core::{NetTick, TickBuffer};
use campfire_net_sync::InputTimelineSetup;
use campfire_net_timeline::{LocalTimeline, LocalTimelineShift};

use crate::prediction_setup::PredictionSetup;

/// The period each local tick ran with, which a rollback replays it with: a rate change inside the
/// rollback window leaves the ticks before it at their own period (11-net-stack/time.md, A rollback
/// replays each tick at the period it ran with).
///
/// Only the ticks where the period changed hold an entry, and a tick before every entry did not
/// run. Without a [`PredictionSetup`] no rollback window exists, so nothing is forgotten.
#[derive(Resource, Debug, Default)]
pub(crate) struct TickPeriodHistory {
    periods: TickBuffer<Duration>,
}

impl TickPeriodHistory {
    /// The period `tick` ran with, or `None` if it did not run.
    pub(crate) fn period(&self, tick: NetTick) -> Option<Duration> {
        let index = self.periods.index_at_or_before(tick)?;
        self.periods.get(index).map(|entry| *entry.value)
    }

    /// The fixed time at the end of `start`, before a replay of the ticks after it up to `end`,
    /// the tick of `now`: `now` less the period each of them ran with.
    pub(crate) fn rewind(&self, now: &Time<Fixed>, start: NetTick, end: NetTick) -> Time<Fixed> {
        let replayed: Duration = (1..=end - start)
            .filter_map(|ticks| self.period(start + ticks))
            .sum();
        let mut time = Time::<Fixed>::from_duration(now.timestep());
        time.advance_to(now.elapsed().saturating_sub(replayed));
        time
    }

    fn record(&mut self, tick: NetTick, period: Duration) {
        if self.period(tick) != Some(period) {
            self.periods.insert(tick, period);
        }
    }

    /// Drops the entries that no tick at or after `oldest` reads.
    fn forget_before(&mut self, oldest: NetTick) {
        let count = self.periods.index_at_or_before(oldest).unwrap_or(0);
        self.periods.drain_front(count);
    }

    fn shift(&mut self, delta: i32) {
        self.periods.shift(delta, |_, _| {});
    }

    /// Records the period of the tick `FixedMain` runs, a replayed one included, and forgets the
    /// ticks before the rollback window.
    pub(crate) fn record_tick(
        timeline: Res<'_, LocalTimeline>,
        time: Res<'_, Time<Fixed>>,
        setup: Option<Res<'_, PredictionSetup>>,
        input_setup: Option<Res<'_, InputTimelineSetup>>,
        mut history: ResMut<'_, Self>,
    ) {
        let tick = timeline.tick();
        history.record(tick, time.delta());
        if let (Some(setup), Some(input_setup)) = (setup, input_setup) {
            history.forget_before(setup.oldest_rollback_tick(tick, &input_setup));
        }
    }

    pub(crate) fn shift_with_local_timeline(
        trigger: On<'_, '_, LocalTimelineShift>,
        mut history: ResMut<'_, Self>,
    ) {
        history.shift(trigger.delta);
    }
}

#[cfg(test)]
mod tests;
