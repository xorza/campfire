use std::sync::Arc;
use std::time::Duration;

use bevy_app::{App, First, Plugin};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use bevy_time::{Time, TimeSystems, Virtual};
use lightyear::core::tick::TickDuration;
use lightyear::core::timeline::SetTickDuration;

use crate::pace::{Pace, PaceSpeed};

/// Applies a local match's `Pace` to an app's clock, its client's or its local server's, at the
/// start of each frame, before the frame's time advances: a pause stops `Time<Virtual>`, so no
/// fixed tick runs; a speed makes the tick length the session's `tick` over it, which Lightyear's
/// `SetTickDuration` gives `Time<Fixed>` and the timelines, and scales the most time a frame
/// advances by as much, so a frame runs as many ticks at most at every speed.
#[derive(Debug)]
pub struct LocalPace {
    pub pace: Arc<Pace>,
    pub tick: Duration,
}

/// The pace an app follows, and the one it applied last.
#[derive(Resource, Debug)]
struct Paced {
    pace: Arc<Pace>,
    tick: Duration,
    paused: bool,
    speed: PaceSpeed,
}

impl Plugin for LocalPace {
    fn build(&self, app: &mut App) {
        app.insert_resource(Paced {
            pace: Arc::clone(&self.pace),
            tick: self.tick,
            paused: false,
            speed: PaceSpeed::Normal,
        });
        app.add_systems(First, Paced::apply.before(TimeSystems));
    }
}

impl Paced {
    fn apply(world: &mut World) {
        let paced = world.resource::<Paced>();
        let (paused, speed) = (paced.pace.paused(), paced.pace.speed());
        let (was_paused, was_speed, tick) = (paced.paused, paced.speed, paced.tick);
        if paused != was_paused {
            let mut time = world.resource_mut::<Time<Virtual>>();
            if paused {
                time.pause();
            } else {
                time.unpause();
            }
        }
        if speed != was_speed {
            let (old, new) = (was_speed.tick(tick), speed.tick(tick));
            world.insert_resource(TickDuration(new));
            world.trigger(SetTickDuration(new));
            let mut time = world.resource_mut::<Time<Virtual>>();
            let scaled = time.max_delta().as_nanos() * new.as_nanos() / old.as_nanos();
            time.set_max_delta(Duration::from_nanos(
                u64::try_from(scaled).expect("a frame's bound fits u64 nanoseconds"),
            ));
        }
        let mut paced = world.resource_mut::<Paced>();
        paced.paused = paused;
        paced.speed = speed;
    }
}
