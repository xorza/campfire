use std::time::{Duration, Instant};

use bevy_ecs::resource::Resource;

use crate::sim_update::SimSystems;

/// The edges around the stages of a tick: before the first, between each two, after the last.
const EDGES: usize = SimSystems::ALL.len() + 1;

/// When the last tick passed each edge between its stages, as the probes `StageClock::install` adds
/// to the sim's schedule note it: what each stage of a real tick costs, its `SimEdgeSystems::After`
/// included, for a bench. The tick's start and end and `SimEdgeSystems::Start` fall outside every
/// stage. It stays outside the sim's state, and the probes touch nothing else, so a match with them
/// plays as one without.
#[derive(Resource, Debug)]
pub struct StageClock {
    edges: [Instant; EDGES],
}

impl StageClock {
    /// How long `stage` took in the last tick.
    pub fn stage(&self, stage: SimSystems) -> Duration {
        let index = SimSystems::ALL
            .iter()
            .position(|&each| each == stage)
            .expect("a stage of the tick");
        self.edges[index + 1].duration_since(self.edges[index])
    }
}

pub(crate) mod internals {
    use std::time::Instant;

    use bevy_ecs::schedule::IntoScheduleConfigs;
    use bevy_ecs::system::ResMut;
    use bevy_ecs::world::World;

    use crate::sim_update::stage_clock::{EDGES, StageClock};
    use crate::sim_update::{SimEdgeSystems, SimSystems, SimUpdate, end_tick};

    impl StageClock {
        /// Adds a probe at each edge of the sim schedule of `world`, after the stage before it
        /// and its `SimEdgeSystems::After` and before the stage after it, and the clock they write.
        pub fn install(world: &mut World) {
            world.insert_resource(Self {
                edges: [Instant::now(); EDGES],
            });
            let probe =
                |edge: usize| move |mut clock: ResMut<'_, Self>| clock.edges[edge] = Instant::now();
            world.schedule_scope(SimUpdate, |_, schedule| {
                schedule.add_systems(
                    probe(0)
                        .after(SimEdgeSystems::Start)
                        .before(SimSystems::Inputs),
                );
                for (edge, stages) in SimSystems::ALL.windows(2).enumerate() {
                    schedule.add_systems(
                        probe(edge + 1)
                            .after(SimEdgeSystems::After(stages[0]))
                            .before(stages[1]),
                    );
                }
                schedule.add_systems(
                    probe(EDGES - 1)
                        .after(SimEdgeSystems::After(SimSystems::Vision))
                        .before(end_tick),
                );
            });
        }
    }
}

#[cfg(test)]
mod tests;
