use std::time::{Duration, Instant};

use bevy_ecs::resource::Resource;

use crate::sim_update::SimSet;

/// The edges around the stages of a tick: before the first, between each two, after the last.
const EDGES: usize = SimSet::ALL.len() + 1;

/// When the last tick passed each edge between its stages, as the probes
/// `SimWorldInternals::install_stage_clock` adds to the sim's schedule note it: what each stage of a real tick costs, its `SimEdge::After` included,
/// for a bench. The tick's start and end and `SimEdge::Start` fall outside every stage. It stays
/// outside the sim's state, and the probes touch nothing else, so a match with them plays as one
/// without.
#[derive(Resource, Debug)]
pub struct StageClock {
    edges: [Instant; EDGES],
}

impl StageClock {
    /// The edges around a tick's stages: before the first, between each two, after the last.
    pub(crate) const EDGES: usize = EDGES;

    /// A clock whose every edge passed now.
    pub(crate) fn started() -> StageClock {
        StageClock {
            edges: [Instant::now(); EDGES],
        }
    }

    /// Notes that the tick passed edge `edge` now.
    pub(crate) fn pass(&mut self, edge: usize) {
        self.edges[edge] = Instant::now();
    }

    /// How long `stage` took in the last tick.
    pub fn stage(&self, stage: SimSet) -> Duration {
        let index = SimSet::ALL
            .iter()
            .position(|&each| each == stage)
            .expect("a stage of the tick");
        self.edges[index + 1].duration_since(self.edges[index])
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
    use bevy_ecs::system::ResMut;
    use bevy_ecs::world::World;
    use campfire_common::SegmentSeed;

    use super::*;
    use crate::sim_update::{SimEdge, SimUpdate};
    use crate::sim_world::SimWorld;
    use crate::sim_world::internals::SimWorldInternals;
    use crate::tick_rate::TickRate;

    /// When each test system ran, with the stage whose time holds it, or none for one before the
    /// first stage.
    #[derive(Resource, Debug, Default)]
    struct Ran(Vec<(Option<SimSet>, Instant)>);

    #[test]
    fn each_stage_runs_between_the_probes_around_it() {
        let mut world = World::new();
        let note = |stage: Option<SimSet>| {
            move |mut ran: ResMut<'_, Ran>| ran.0.push((stage, Instant::now()))
        };
        let rate = TickRate::new(NonZeroU32::new(30).unwrap());
        world.sim_builder(SegmentSeed::new([7; 32]), rate).build();
        world.init_resource::<Ran>();
        world.resource_mut::<Schedules>().add_systems(
            SimUpdate,
            (
                note(None).in_set(SimEdge::Start),
                note(Some(SimSet::Think)).in_set(SimSet::Think),
                note(Some(SimSet::Think)).in_set(SimEdge::After(SimSet::Think)),
                note(Some(SimSet::Move)).in_set(SimSet::Move),
                note(Some(SimSet::Vision)).in_set(SimEdge::After(SimSet::Vision)),
            ),
        );
        world.install_stage_clock();
        let installed = Instant::now();
        world.run_schedule(SimUpdate);

        let clock = world.resource::<StageClock>();
        // Every probe ran in this tick, in the order of the stages.
        assert!(clock.edges.iter().all(|&edge| edge >= installed));
        assert!(clock.edges.is_sorted());
        // Each system ran between the probes around its stage, the one in the gap after a stage
        // with that stage: Think is the second stage, Move the fourth and Vision the ninth. The
        // gap before the first stage falls outside every stage.
        let ran = &world.resource::<Ran>().0;
        assert_eq!(ran.len(), 5);
        for &(stage, at) in ran {
            match stage {
                None => assert!(installed <= at && at <= clock.edges[0]),
                Some(stage) => {
                    let index = SimSet::ALL.iter().position(|&each| each == stage).unwrap();
                    assert!(clock.edges[index] <= at && at <= clock.edges[index + 1]);
                }
            }
        }
        // The stages split the tick between the first probe and the last, whole.
        let total: Duration = SimSet::ALL.iter().map(|&stage| clock.stage(stage)).sum();
        assert_eq!(total, clock.edges[EDGES - 1].duration_since(clock.edges[0]));
    }
}
