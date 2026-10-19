use crate::local_timeline::{LocalTimeline, end_local_tick, increment_local_tick};
use crate::set_tick_rate::SetTickRate;
use crate::timeline_systems::TimelineSystems;
use bevy_app::{App, FixedFirst, FixedLast, FixedMainScheduleOrder, Plugin};
use bevy_ecs::prelude::On;
use bevy_ecs::schedule::{IntoScheduleConfigs, ScheduleLabel};
use bevy_ecs::system::ResMut;
use bevy_time::{Fixed, Time};
use campfire_net_core::TickRate;

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
        mut tick_rate: ResMut<'_, TickRate>,
        mut time: ResMut<'_, Time<Fixed>>,
    ) {
        *tick_rate = trigger.0;
        time.set_timestep(trigger.0.period());
    }
}

impl Plugin for LocalTimelinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<LocalTimeline>();
        app.insert_resource(self.tick_rate);
        app.world_mut()
            .resource_mut::<Time<Fixed>>()
            .set_timestep(self.tick_rate.period());
        app.add_observer(Self::update_tick_rate);

        app.add_systems(
            FixedFirst,
            increment_local_tick.in_set(TimelineSystems::IncrementLocal),
        );
        app.add_systems(EndLocalTick, end_local_tick);
        app.world_mut()
            .get_resource_or_init::<FixedMainScheduleOrder>()
            .insert_after(FixedLast, EndLocalTick);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_app::{FixedMain, FixedUpdate};
    use bevy_ecs::prelude::Resource;
    use bevy_ecs::system::Res;
    use bevy_time::TimePlugin;
    use campfire_net_core::NetTick;

    fn rate(hz: u16) -> TickRate {
        TickRate::new(core::num::NonZeroU16::new(hz).unwrap())
    }

    /// The event moves the rate from 64 Hz to 50 Hz, and the timestep with it, to 20 ms.
    #[test]
    fn set_tick_rate_updates_the_rate_and_the_timestep() {
        let mut app = App::new();
        app.add_plugins((
            TimePlugin,
            LocalTimelinePlugin {
                tick_rate: rate(64),
            },
        ));
        assert_eq!(
            app.world().resource::<Time<Fixed>>().timestep(),
            core::time::Duration::from_micros(15_625)
        );
        app.world_mut().trigger(SetTickRate(rate(50)));

        assert_eq!(*app.world().resource::<TickRate>(), rate(50));
        assert_eq!(
            app.world().resource::<Time<Fixed>>().timestep(),
            core::time::Duration::from_millis(20)
        );
    }

    #[derive(Resource, Default)]
    struct Seen(Vec<Option<NetTick>>);

    /// `FixedMain` simulates the tick it advanced to, 1 and then 2, in `FixedUpdate` and in
    /// `FixedLast` alike; before and after it, no tick is simulated.
    #[test]
    fn the_simulated_tick_is_the_one_fixed_main_runs() {
        let mut app = App::new();
        app.add_plugins((
            TimePlugin,
            LocalTimelinePlugin {
                tick_rate: rate(64),
            },
        ));
        app.init_resource::<Seen>();
        let record = |timeline: Res<'_, LocalTimeline>, mut seen: ResMut<'_, Seen>| {
            seen.0.push(timeline.simulated_tick());
        };
        app.add_systems(FixedUpdate, record);
        app.add_systems(FixedLast, record);
        let simulated = |app: &App| app.world().resource::<LocalTimeline>().simulated_tick();

        assert_eq!(simulated(&app), None);
        app.world_mut().run_schedule(FixedMain);
        assert_eq!(simulated(&app), None);
        app.world_mut().run_schedule(FixedMain);
        assert_eq!(simulated(&app), None);

        assert_eq!(
            app.world().resource::<Seen>().0,
            [
                Some(NetTick(1)),
                Some(NetTick(1)),
                Some(NetTick(2)),
                Some(NetTick(2))
            ]
        );
    }
}
