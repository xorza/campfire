use std::sync::Arc;
use std::time::Duration;

use bevy_app::{App, First, Plugin};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::world::World;
use bevy_time::{Time, TimeSystems, Virtual};
use campfire_math::TickRate;
use campfire_net_timeline::SetTickRate;

use campfire_host::{Pace, PaceSpeed};

/// Applies a local match's `Pace` to an app's clock, its client's or its local server's, at the
/// start of each frame, before the frame's time advances: a pause stops `Time<Virtual>`, so no
/// fixed tick runs; a speed makes the timelines' rate the session's `rate` times it, which the
/// stack's `SetTickRate` gives `Time<Fixed>` and the timelines while the sim keeps the session's,
/// and scales the most time a frame advances by as much, so a frame runs as many ticks at most at
/// every speed (08-sessions.md, The pace sets the tick rate).
#[derive(Debug)]
pub struct LocalPacePlugin {
    pub pace: Arc<Pace>,
    pub tick_rate: TickRate,
}

/// The pace an app follows, and the one it applied last.
#[derive(Resource, Debug)]
struct Paced {
    pace: Arc<Pace>,
    tick_rate: TickRate,
    paused: bool,
    speed: PaceSpeed,
}

impl Plugin for LocalPacePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Paced {
            pace: Arc::clone(&self.pace),
            tick_rate: self.tick_rate,
            paused: false,
            speed: PaceSpeed::Normal,
        });
        app.add_systems(First, Paced::apply.before(TimeSystems));
    }
}

impl Paced {
    fn apply(world: &mut World) {
        let paced = world.resource::<Self>();
        let (paused, speed) = (paced.pace.is_paused(), paced.pace.speed());
        let (was_paused, was_speed, rate) = (paced.paused, paced.speed, paced.tick_rate);
        if paused != was_paused {
            let mut time = world.resource_mut::<Time<Virtual>>();
            if paused {
                time.pause();
            } else {
                time.unpause();
            }
        }
        if speed != was_speed {
            let (old, new) = (was_speed.rate(rate), speed.rate(rate));
            world.trigger(SetTickRate(new));
            let mut time = world.resource_mut::<Time<Virtual>>();
            let scaled =
                time.max_delta().as_nanos() * new.period().as_nanos() / old.period().as_nanos();
            time.set_max_delta(Duration::from_nanos(
                u64::try_from(scaled).expect("a frame's bound fits u64 nanoseconds"),
            ));
        }
        let mut paced = world.resource_mut::<Self>();
        paced.paused = paused;
        paced.speed = speed;
    }
}
