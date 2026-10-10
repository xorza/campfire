use std::hint::black_box;

use campfire_math::{Num, Vec3};
use campfire_sim::Position;
use criterion::{Criterion, Throughput};

use crate::geometry::bounds::Bounds;
use crate::geometry::grid::Grid;
use crate::geometry::kernel_scene::{Density, KernelScene};
use crate::units::relations::Relations;
use crate::units::team::Team;
use crate::vision::brush_map::BrushMap;
use crate::vision::fog::Fog;
use crate::vision::vision_grid::VisionGrid;

/// Each unit's sight in meters: the MOBA's units see 8 to 11 m.
const SIGHT: i64 = 10;

/// The Vision stage's grid fog for `KernelScene::UNITS` units of two teams, as `vision::see`
/// runs it: each unit reveals the cells within its 10 m sight on a grid of 1 m cells with no
/// brush, then each learns the teams that see it. Each sight reveals as many cells however close
/// the units stand, so one density serves, the spread one; the grid reaches a sight past it, so
/// no sight is cut at its edge. In `sight` every unit moved since the tick before, a bit along x
/// and back by turns, so each finds its sight's runs on the grid; in `still` none did, so each
/// takes its runs again.
pub(crate) fn fog(c: &mut Criterion) {
    let span = Density::Spread.span();
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

    let moved: Vec<(Position, Team)> = units
        .iter()
        .map(|&(at, team)| {
            let step = Vec3::new(Num::EPSILON, Num::ZERO, Num::ZERO);
            (Position::new(at.get() + step).unwrap(), team)
        })
        .collect();
    let mut tick = |fog: &mut Fog, units: &[(Position, Team)]| {
        fog.begin_tick();
        for (slot, &(pos, team)) in units.iter().enumerate() {
            fog.sight(&vision, slot, pos, team, range, false);
        }
        seen.clear();
        for (slot, &(pos, team)) in units.iter().enumerate() {
            seen.push(fog.seen_by(&vision, slot, pos, None, team, false));
        }
        black_box(&seen);
    };

    let mut group = c.benchmark_group("integration/fog");
    group.throughput(Throughput::Elements(KernelScene::UNITS as u64));
    let mut turn = false;
    group.bench_function("sight", |bench| {
        bench.iter(|| {
            turn = !turn;
            tick(&mut fog, if turn { &moved } else { &units });
        });
    });
    group.bench_function("still", |bench| {
        bench.iter(|| tick(&mut fog, &units));
    });
    group.finish();
}
