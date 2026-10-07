use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use campfire_capabilities::Team;
use campfire_capabilities::internals::{Density, KernelScene, read_view, spawn_typed};
use campfire_sim::Position;
use criterion::{Criterion, Throughput};

use crate::harness::reference_3v3::Reference3v3;

/// The creep types of the 3v3's waves, which the scene's units take in turn.
const CREEPS: [&str; 2] = ["melee_creep", "caster_creep"];
/// A later read of a tick finds one unit in this many changed.
const REREAD_CHANGED: usize = 100;

/// The script view's read, as each batch of script calls reads it: the reference 3v3 at its
/// start with `KernelScene::UNITS` of its creeps more, on both teams at the crowded scene's
/// points, which lie within its map, each read giving every unit a row with the column of each
/// capability the 3v3 declares, filled again where the unit's parts changed. `read` follows a
/// move of every unit, as a tick's first batch finds them; `reread` a move of one in
/// `REREAD_CHANGED`, as a later batch of the same tick does.
pub(crate) fn script_view(c: &mut Criterion) {
    let reference = Reference3v3::load();
    let mut fixed = reference.start();
    let world = fixed.runner_mut().world_mut();
    let mut scene = KernelScene::new(13);
    for unit in 0..KernelScene::UNITS {
        let at = Position::new(scene.point(Density::Crowded.span())).unwrap();
        let team = Team::new(u8::from(unit % 4 >= 2));
        spawn_typed(world, CREEPS[unit % 2], team, at);
    }
    let units: Vec<Entity> = world
        .query_filtered::<Entity, (With<Position>, With<Team>)>()
        .iter(world)
        .collect();

    let mut group = c.benchmark_group("script_view");
    group.throughput(Throughput::Elements(units.len() as u64));
    for (case, every) in [("read", 1), ("reread", REREAD_CHANGED)] {
        let mut round = 0;
        group.bench_function(case, |b| {
            b.iter_custom(|reads| {
                let mut spent = Duration::ZERO;
                for _ in 0..reads {
                    for &unit in units.iter().skip(round % every).step_by(every) {
                        world.get_mut::<Position>(unit).unwrap().set_changed();
                    }
                    round += 1;
                    let start = Instant::now();
                    read_view(world);
                    spent += start.elapsed();
                }
                black_box(&*world);
                spent
            });
        });
    }
    group.finish();
}
