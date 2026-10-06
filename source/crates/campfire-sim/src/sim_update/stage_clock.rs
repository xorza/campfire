use std::time::{Duration, Instant};

use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::ResMut;
use bevy_ecs::world::World;

use crate::sim_update::{SimEdge, SimSet, SimUpdate, end_tick, start_tick};

/// The edges around the stages of a tick: before the first, between each two, after the last.
const EDGES: usize = SimSet::ALL.len() + 1;

/// When the last tick passed each edge between its stages, as the probes `install` adds to the
/// sim's schedule note it: what each stage of a real tick costs, its `SimEdge::After` included,
/// for a bench. The tick's start and end and `SimEdge::Start` fall outside every stage. It stays
/// outside the sim's state, and the probes touch nothing else, so a match with them plays as one
/// without.
#[derive(Resource, Debug)]
pub struct StageClock {
    edges: [Instant; EDGES],
}

impl StageClock {
    /// Adds a probe at each edge of `world`'s `SimUpdate` schedule, after the stage before it and
    /// its `SimEdge::After` and before the stage after it, and the clock they write.
    pub fn install(world: &mut World) {
        world.insert_resource(StageClock {
            edges: [Instant::now(); EDGES],
        });
        let probe = |edge: usize| {
            move |mut clock: ResMut<'_, StageClock>| {
                clock.edges[edge] = Instant::now();
            }
        };
        world.schedule_scope(SimUpdate, |_, schedule| {
            schedule.add_systems(
                probe(0)
                    .after(start_tick)
                    .after(SimEdge::Start)
                    .before(SimSet::Inputs),
            );
            for (edge, stages) in SimSet::ALL.windows(2).enumerate() {
                schedule.add_systems(
                    probe(edge + 1)
                        .after(SimEdge::After(stages[0]))
                        .before(stages[1]),
                );
            }
            schedule.add_systems(
                probe(EDGES - 1)
                    .after(SimEdge::After(SimSet::Vision))
                    .before(end_tick),
            );
        });
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

    use campfire_common::SegmentSeed;

    use super::*;
    use crate::tick_rate::TickRate;

    /// When each test system ran, with the stage whose time holds it, or none for one before the
    /// first stage.
    #[derive(Resource, Debug, Default)]
    struct Ran(Vec<(Option<SimSet>, Instant)>);

    #[test]
    fn each_stage_runs_between_the_probes_around_it() {
        let mut world = World::new();
        SimUpdate::prepare(
            &mut world,
            SegmentSeed::new([7; 32]),
            TickRate::new(NonZeroU32::new(30).unwrap()),
        );
        world.init_resource::<Ran>();
        let mut schedule = SimUpdate::schedule();
        let note = |stage: Option<SimSet>| {
            move |mut ran: ResMut<'_, Ran>| ran.0.push((stage, Instant::now()))
        };
        schedule.add_systems((
            note(None).in_set(SimEdge::Start),
            note(Some(SimSet::Think)).in_set(SimSet::Think),
            note(Some(SimSet::Think)).in_set(SimEdge::After(SimSet::Think)),
            note(Some(SimSet::Move)).in_set(SimSet::Move),
            note(Some(SimSet::Vision)).in_set(SimEdge::After(SimSet::Vision)),
        ));
        world.add_schedule(schedule);
        StageClock::install(&mut world);
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
