use std::hint::black_box;

use campfire_math::Num;
use campfire_sim::Position;
use criterion::{Criterion, Throughput};

use crate::units::relations::Relations;
use crate::units::team::Team;
use crate::values::bounds::Bounds;
use crate::values::grid::Grid;
use crate::values::kernel_scene::KernelScene;
use crate::vision::brush_map::BrushMap;
use crate::vision::fog::Fog;
use crate::vision::vision_grid::VisionGrid;

/// Each unit's sight in meters: the reference units see 8 to 11 m.
const SIGHT: i64 = 10;

/// The Vision stage's grid fog for `KernelScene::UNITS` units of two teams, as `vision::see` runs it:
/// each unit reveals the cells within its 10 m sight on a grid of 1 m cells with no brush, then
/// each learns the teams that see it. Each sight reveals as many cells however close the units
/// stand, so one scene serves, the spread one; the grid reaches a sight past it, so no sight is
/// cut at its edge.
pub(crate) fn fog(c: &mut Criterion) {
    let (_, span) = KernelScene::SPREAD;
    let mut scene = KernelScene::new(11);
    let units: Vec<(Position, Team)> = (0..KernelScene::UNITS)
        .map(|_| {
            let at = Position::new(scene.point(span)).unwrap();
            let team = Team::new(u8::try_from(scene.below(2)).unwrap());
            (at, team)
        })
        .collect();
    let edge = Num::from_int(span.cast_signed() + SIGHT).unwrap();
    let bounds = Bounds::new([-edge; 2], [edge; 2]).unwrap();
    let grid = Grid::new(Num::ONE, bounds).unwrap();
    let vision = VisionGrid {
        brush: BrushMap::new(&grid, &[]),
        grid,
        teams: 2,
    };
    let range = Num::from_int(SIGHT).unwrap();
    let mut fog = Fog::default();
    fog.rebuild(&vision, &Relations::default());
    let mut seen = Vec::with_capacity(KernelScene::UNITS);

    let mut group = c.benchmark_group("fog");
    group.throughput(Throughput::Elements(KernelScene::UNITS as u64));
    group.bench_function("sight", |bench| {
        bench.iter(|| {
            fog.begin_tick();
            for &(pos, team) in &units {
                fog.sight(&vision, pos, team, range, false);
            }
            seen.clear();
            seen.extend(
                units
                    .iter()
                    .map(|&(pos, team)| fog.seen_by(&vision, pos, team, false)),
            );
            black_box(&seen);
        });
    });
    group.finish();
}
