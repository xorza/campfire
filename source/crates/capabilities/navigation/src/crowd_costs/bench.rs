use std::hint::black_box;

use campfire_geometry::internals::{Density, KernelScene};
use campfire_math::{Num, Vec3};
use campfire_measure::{Kind, Scale};
use campfire_units::Layer;
use campfire_values::Team;
use criterion::Criterion;

use crate::bench::scene_cells;
use crate::broadphase::internals::{indexed, scene};
use crate::crowd_costs::{CrowdCosts, Stamp, Standing};
use crate::walker::Walker;
use crate::walker_kinds::WalkerKinds;

/// The changes of an update: a few units, a group's, and every unit.
const CHANGES: [usize; 3] = [10, 100, KernelScene::UNITS];

/// The crowd's counts of the crowded scene's `KernelScene::UNITS` units, all standing, of teams 0
/// and 1 in turn, for a walker of a half meter, as navigation counts them each tick before routes
/// plan: an update in which `n` of them stand a meter east of where they stood, `update_<n>_changes`,
/// its throughput the changes (04-capabilities/navigation.md, A route prices the units on it).
pub(crate) fn crowd_costs(c: &mut Criterion) {
    let span = Density::Crowded.span();
    let stamps: Vec<Stamp> = scene(9, KernelScene::UNITS, span, 1, false)
        .iter()
        .enumerate()
        .map(|(nth, body)| Stamp {
            body: indexed(body),
            team: Some(Team::new(u8::try_from(nth % 2).unwrap())),
            standing: if body.movable {
                Standing::Movable
            } else {
                Standing::Held
            },
        })
        .collect();
    let walker = Walker {
        layer: Layer::FIRST,
        radius: Num::HALF,
    };
    let mut costs = CrowdCosts::new(scene_cells(span), &WalkerKinds::from([walker]));
    let mut group = Kind::Atomic.group(c, "crowd_costs", Scale::Sample);
    for changes in CHANGES {
        let moved: Vec<Stamp> = (0..)
            .zip(&stamps)
            .map(|(nth, &stamp)| {
                let mut stamp = stamp;
                if nth < changes {
                    stamp.body.position += Vec3::from((1, 0, 0));
                }
                stamp
            })
            .collect();
        let sides = [&stamps, &moved];
        let mut side = 0;
        costs.update(stamps.iter().copied());
        group.throughput(campfire_measure::elements(changes));
        group.bench_function(
            campfire_measure::case("update", changes, "change"),
            |bench| {
                bench.iter(|| {
                    side = 1 - side;
                    costs.update(black_box(sides[side].iter().copied()));
                });
            },
        );
    }
    group.finish();
}
