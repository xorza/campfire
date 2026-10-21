use std::num::NonZeroU32;

use bevy_ecs::lifecycle::RemovedComponents;
use bevy_ecs::query::With;
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Query};
use campfire_common::SegmentSeed;

use super::*;
use crate::match_rate::MatchRate;
use crate::sim_update::{SimSystems, SimUpdate};

#[derive(Component, Debug)]
struct Mark;

/// What the reader read at each tick: through `Removals`, and through Bevy's
/// `RemovedComponents`.
#[derive(Resource, Debug, Default)]
struct Seen {
    by_tick: Vec<usize>,
    by_app: Vec<usize>,
}

fn read(
    mut removals: Removals<'_, '_, Mark>,
    mut app: RemovedComponents<'_, '_, Mark>,
    mut seen: ResMut<'_, Seen>,
) {
    seen.by_tick.push(removals.read().count());
    seen.by_app.push(app.read().count());
}

/// Removes one mark late in the tick, after the reader ran.
fn remove_one(marked: Query<'_, '_, Entity, With<Mark>>, mut commands: Commands<'_, '_>) {
    if let Some(entity) = marked.iter().min() {
        commands.entity(entity).remove::<Mark>();
    }
}

/// Four ticks of a world whose host clears its trackers `updates` times after each tick, as
/// an app that runs that many frames a tick does.
fn host(updates: u32) -> Seen {
    let mut world = World::new();
    let rate = MatchRate::new(NonZeroU32::new(30).unwrap());
    SimUpdate::prepare(&mut world, SegmentSeed::new([1; 32]), rate);
    world.init_resource::<Seen>();
    let mut schedule = SimUpdate::schedule();
    Removals::<Mark>::track(&mut world, &mut schedule);
    Removals::<Mark>::track(&mut world, &mut schedule);
    schedule.add_systems((
        read.in_set(SimSystems::Inputs),
        remove_one.in_set(SimSystems::Mode),
    ));
    world.spawn_batch([Mark, Mark, Mark]);
    for _ in 0..4 {
        schedule.run(&mut world);
        for _ in 0..updates {
            world.clear_trackers();
        }
    }
    world.remove_resource::<Seen>().unwrap()
}

#[test]
fn a_removal_after_its_reader_reaches_it_next_tick_however_often_the_host_updates() {
    // Each tick removes one mark after the reader ran, so the reader of each tick but the
    // first reads one removal, that of the tick before, whatever the host does between ticks.
    let replay = host(1);
    let server = host(3);
    assert_eq!(replay.by_tick, [0, 1, 1, 1]);
    assert_eq!(server.by_tick, replay.by_tick);
    // Bevy's own removals reach a replay's reader, which clears once a tick, and are gone
    // from a server's, which clears three times: the divergence the tick's removals end.
    assert_eq!(replay.by_app, [0, 1, 1, 1]);
    assert_eq!(server.by_app, [0, 0, 0, 0]);
}
