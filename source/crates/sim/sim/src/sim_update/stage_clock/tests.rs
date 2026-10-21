use std::num::NonZeroU32;

use bevy_ecs::schedule::{IntoScheduleConfigs, Schedules};
use bevy_ecs::system::ResMut;
use bevy_ecs::world::World;
use campfire_common::SegmentSeed;

use super::*;
use crate::match_rate::MatchRate;
use crate::sim_update::{SimEdgeSystems, SimUpdate};
use crate::sim_world_builder::SimWorldBuilder;

/// When each test system ran, with the stage whose time holds it, or none for one before the
/// first stage.
#[derive(Resource, Debug, Default)]
struct Ran(Vec<(Option<SimSystems>, Instant)>);

#[test]
fn each_stage_runs_between_the_probes_around_it() {
    let mut world = World::new();
    let note = |stage: Option<SimSystems>| {
        move |mut ran: ResMut<'_, Ran>| ran.0.push((stage, Instant::now()))
    };
    let rate = MatchRate::new(NonZeroU32::new(30).unwrap());
    SimWorldBuilder::new(&mut world, SegmentSeed::new([7; 32]), rate).build();
    world.init_resource::<Ran>();
    world.resource_mut::<Schedules>().add_systems(
        SimUpdate,
        (
            note(None).in_set(SimEdgeSystems::Start),
            note(Some(SimSystems::Think)).in_set(SimSystems::Think),
            note(Some(SimSystems::Think)).in_set(SimEdgeSystems::After(SimSystems::Think)),
            note(Some(SimSystems::Move)).in_set(SimSystems::Move),
            note(Some(SimSystems::Vision)).in_set(SimEdgeSystems::After(SimSystems::Vision)),
        ),
    );
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
                let index = SimSystems::ALL
                    .iter()
                    .position(|&each| each == stage)
                    .unwrap();
                assert!(clock.edges[index] <= at && at <= clock.edges[index + 1]);
            }
        }
    }
    // The stages split the tick between the first probe and the last, whole.
    let total: Duration = SimSystems::ALL
        .iter()
        .map(|&stage| clock.stage(stage))
        .sum();
    assert_eq!(total, clock.edges[EDGES - 1].duration_since(clock.edges[0]));
}
