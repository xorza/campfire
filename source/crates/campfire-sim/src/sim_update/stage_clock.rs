use std::time::{Duration, Instant};

use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::IntoScheduleConfigs;
use bevy_ecs::system::ResMut;
use bevy_ecs::world::World;

use crate::sim_update::{SimSet, SimUpdate};

/// The edges around the stages of a tick: before the first, between each two, after the last.
const EDGES: usize = SimSet::ALL.len() + 1;

/// When the last tick passed each edge between its stages, as the probes `install` adds to the
/// sim's schedule note it: what each stage of a real tick costs, for a bench. It stays outside the
/// sim's state, and the probes touch nothing else, so a match with them plays as one without.
#[derive(Resource, Debug)]
pub struct StageClock {
    edges: [Instant; EDGES],
}

impl StageClock {
    /// Adds a probe at each edge of `world`'s `SimUpdate` schedule, ordered between the two
    /// stages it lies between, and the clock they write.
    pub fn install(world: &mut World) {
        world.insert_resource(StageClock {
            edges: [Instant::now(); EDGES],
        });
        world.schedule_scope(SimUpdate, |_, schedule| {
            for edge in 0..EDGES {
                let probe = move |mut clock: ResMut<'_, StageClock>| {
                    clock.edges[edge] = Instant::now();
                };
                let before = edge.checked_sub(1).map(|stage| SimSet::ALL[stage]);
                match (before, SimSet::ALL.get(edge)) {
                    (Some(before), Some(&after)) => {
                        schedule.add_systems(probe.after(before).before(after))
                    }
                    (None, Some(&after)) => schedule.add_systems(probe.before(after)),
                    (Some(before), None) => schedule.add_systems(probe.after(before)),
                    (None, None) => unreachable!("a tick has stages"),
                };
            }
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

    /// When each test system ran, with its stage.
    #[derive(Resource, Debug, Default)]
    struct Ran(Vec<(SimSet, Instant)>);

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
        let note =
            |stage: SimSet| move |mut ran: ResMut<'_, Ran>| ran.0.push((stage, Instant::now()));
        schedule.add_systems((
            note(SimSet::Think).in_set(SimSet::Think),
            note(SimSet::Move).in_set(SimSet::Move),
        ));
        world.add_schedule(schedule);
        StageClock::install(&mut world);
        let installed = Instant::now();
        world.run_schedule(SimUpdate);

        let clock = world.resource::<StageClock>();
        // Every probe ran in this tick, in the order of the stages.
        assert!(clock.edges.iter().all(|&edge| edge >= installed));
        assert!(clock.edges.is_sorted());
        // Each system ran between the probes around its stage: Think is the second stage, Move the
        // fourth.
        let ran = &world.resource::<Ran>().0;
        assert_eq!(ran.len(), 2);
        for &(stage, at) in ran {
            let index = SimSet::ALL.iter().position(|&each| each == stage).unwrap();
            assert!(clock.edges[index] <= at && at <= clock.edges[index + 1]);
        }
        // The stages split the tick between the first probe and the last, whole.
        let total: Duration = SimSet::ALL.iter().map(|&stage| clock.stage(stage)).sum();
        assert_eq!(total, clock.edges[EDGES - 1].duration_since(clock.edges[0]));
    }
}
