use std::hint::black_box;
use std::time::{Duration, Instant};

use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use campfire_geometry::internals::KernelScene;
use campfire_measure::{Kind, Scale};
use campfire_script::internals::ScriptsWorldInternals;
use campfire_sim::Position;
use campfire_values::Team;
use criterion::{Criterion, Throughput};

use crate::fixed_session::fixed_match::FixedMatch;
use crate::moba_3v3::Moba3v3;

/// A later read of a tick finds one unit in this many changed.
const REREAD_CHANGED: usize = 100;

/// The 3v3 at its start with `KernelScene::UNITS` of its creeps more, and every unit of it.
#[derive(Debug)]
struct ViewScene {
    fixed: FixedMatch,
    units: Vec<Entity>,
}

impl ViewScene {
    fn new() -> Self {
        let mut fixed = Moba3v3::load().start();
        let world = fixed.game_mut();
        Moba3v3::crowd_creeps(world);
        let units = world
            .query_filtered::<Entity, (With<Position>, With<Team>)>()
            .iter(world)
            .collect();
        Self { fixed, units }
    }
}

/// The script view's read, as each batch of script calls reads it: the MOBA 3v3 at its
/// start with `KernelScene::UNITS` of its creeps more, on both teams at the crowded scene's
/// points, which lie within its map, each read giving every unit a row with the column of each
/// capability the 3v3 declares, filled again where the unit's parts changed. `read` follows a
/// move of every unit, as a tick's first batch finds them; `reread` a move of one in
/// `REREAD_CHANGED`, as a later batch of the same tick does. Its throughput is the scene's
/// creeps, which the 3v3's own few units join.
pub(crate) fn script_view(c: &mut Criterion) {
    let mut scene = None;
    let mut group = Kind::Integration.group(c, "script_view", Scale::Sample);
    group.throughput(Throughput::Elements(KernelScene::UNITS as u64));
    for (case, every) in [("read", 1), ("reread", REREAD_CHANGED)] {
        let mut round = 0;
        group.bench_function(case, |b| {
            let ViewScene { fixed, units } = scene.get_or_insert_with(ViewScene::new);
            let world = fixed.game_mut();
            b.iter_custom(|reads| {
                let mut spent = Duration::ZERO;
                for _ in 0..reads {
                    for &unit in units.iter().skip(round % every).step_by(every) {
                        world.get_mut::<Position>(unit).unwrap().set_changed();
                    }
                    round += 1;
                    let start = Instant::now();
                    world.read_view();
                    spent += start.elapsed();
                }
                black_box(&*world);
                spent
            });
        });
    }
    group.finish();
}
