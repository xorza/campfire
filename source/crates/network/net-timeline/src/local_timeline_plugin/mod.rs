use crate::local_timeline::LocalTimeline;
use crate::set_tick_rate::SetTickRate;
use crate::timeline_rate::TimelineRate;
use bevy_app::{App, FixedFirst, FixedLast, FixedMainScheduleOrder, Plugin};
use bevy_ecs::prelude::On;
use bevy_ecs::schedule::ScheduleLabel;
use bevy_ecs::system::ResMut;
use bevy_time::{Fixed, Time};
use campfire_math::TickRate;

/// Runs after `FixedLast` in each `FixedMain`, a rollback replay's included, where the local tick
/// ends: a schedule of its own, as no system of `FixedLast` is ordered after all the others.
#[derive(ScheduleLabel, Debug, Clone, PartialEq, Eq, Hash)]
struct EndLocalTick;

#[derive(Debug)]
pub(crate) struct LocalTimelinePlugin {
    pub(crate) tick_rate: TickRate,
}

impl LocalTimelinePlugin {
    /// The one writer of the timestep, which it derives from the rate.
    fn update_tick_rate(
        trigger: On<'_, '_, SetTickRate>,
        mut tick_rate: ResMut<'_, TimelineRate>,
        mut time: ResMut<'_, Time<Fixed>>,
    ) {
        *tick_rate = TimelineRate(trigger.0);
        time.set_timestep(trigger.0.period());
    }
}

impl Plugin for LocalTimelinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalTimeline>();
        app.insert_resource(TimelineRate(self.tick_rate));
        app.world_mut()
            .resource_mut::<Time<Fixed>>()
            .set_timestep(self.tick_rate.period());
        app.add_observer(Self::update_tick_rate);

        app.add_systems(FixedFirst, LocalTimeline::increment_tick);
        app.add_systems(EndLocalTick, LocalTimeline::end_tick);
        app.world_mut()
            .get_resource_or_init::<FixedMainScheduleOrder>()
            .insert_after(FixedLast, EndLocalTick);
    }
}

#[cfg(test)]
mod tests;
