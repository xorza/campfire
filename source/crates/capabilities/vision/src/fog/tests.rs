use campfire_geometry::internals::KernelScene;
use campfire_geometry::{Bounds, Grid, Polygon};

use campfire_common::Ticks;

use super::*;
use crate::brush_map::BrushMap;
use crate::reveals::Reveal;

/// A grid of 1 m cells over ±50 m, three teams with no allies, so three groups, and two brushes:
/// a square from (−6, −6) to (4, 5) and a triangle by (8, 8).
fn vision() -> VisionGrid {
    let edge = Num::int(50);
    let grid = Grid::new(Num::ONE, Bounds::new([-edge; 2], [edge; 2]).unwrap()).unwrap();
    let corner = |x: i64, z: i64| [Num::int(x), Num::int(z)];
    let brushes = [
        Polygon::new([corner(-6, -6), corner(4, -6), corner(4, 5), corner(-6, 5)]).unwrap(),
        Polygon::new([corner(8, 8), corner(16, 8), corner(16, 12)]).unwrap(),
    ];
    VisionGrid {
        brush: BrushMap::new(&grid, &brushes),
        grid,
        teams: 3,
    }
}

/// Each group's sight and detection, cell by cell, as a fog built afresh from `seers` and
/// `reveals` sees them: a seer reveals the cells within its range but those of every brush other
/// than its own, a reveal every cell within its radius.
fn afresh(
    fog: &Fog,
    vision: &VisionGrid,
    seers: &[Seer],
    reveals: &[(Vec3, Team, Num)],
) -> Vec<[bool; 2]> {
    let cells = vision.grid.cells();
    let mut seen = vec![[false; 2]; cells * fog.groups.count()];
    for seer in seers {
        let group = fog.groups.of(seer.team);
        let stands = vision.grid.cell_of(seer.position).unwrap();
        let hidden = vision.brush.hidden_in(vision.brush.brush_of(stands));
        vision.grid.spans_within(seer.position, seer.range, |run| {
            for cell in run {
                if hidden.is_none_or(|hidden| (hidden.word(cell / 64) >> (cell % 64)) & 1 == 0) {
                    seen[group * cells + cell][0] = true;
                    seen[group * cells + cell][1] |= seer.detects;
                }
            }
        });
    }
    for &(position, team, radius) in reveals {
        let group = fog.groups.of(team);
        vision.grid.spans_within(position, radius, |run| {
            for cell in run {
                seen[group * cells + cell][0] = true;
            }
        });
    }
    seen
}

/// Each group's sight and detection, cell by cell, as `fog` holds them.
fn held(fog: &Fog, vision: &VisionGrid) -> Vec<[bool; 2]> {
    let cells = vision.grid.cells();
    (0..fog.groups.count() * cells)
        .map(|at| {
            let (group, cell) = (at / cells, at % cells);
            [false, true].map(|hidden| fog.maps.sees(group, cell..cell + 1, hidden))
        })
        .collect()
}

#[test]
fn a_counted_fog_sees_what_a_fog_built_afresh_each_tick_sees() {
    // 40 slots over 200 ticks: each tick a slot's seer may leave, come, change team, or move up
    // to half a meter either way, and every seventh tick reveals 3 m to team 1 for 10 ticks, so
    // two reveals overlap at times and each ends by itself.
    let vision = vision();
    let mut scene = KernelScene::new(5);
    let seer = |scene: &mut KernelScene, slot: usize| Seer {
        slot,
        position: scene.point(20),
        team: Team::new(u8::try_from(scene.below(3)).unwrap()),
        range: Num::int(2 + i64::try_from(scene.below(8)).unwrap()),
        detects: scene.below(4) == 0,
    };
    let mut slots: Vec<Option<Seer>> = (0..40).map(|slot| Some(seer(&mut scene, slot))).collect();
    let mut fog = Fog::default();
    fog.rebuild(&vision, &Relations::default());
    let mut held_reveals = Reveals::default();
    for tick in 0..200 {
        for (slot, unit) in slots.iter_mut().enumerate() {
            match scene.below(12) {
                0 => *unit = None,
                1 => *unit = Some(seer(&mut scene, slot)),
                2 => {
                    if let Some(unit) = unit {
                        unit.team = Team::new(u8::try_from(scene.below(3)).unwrap());
                    }
                }
                3..=7 => {
                    if let Some(unit) = unit {
                        let step = |scene: &mut KernelScene| {
                            Num::from_bits(i64::try_from(scene.below(1 << 23)).unwrap() - (1 << 22))
                        };
                        let moved = unit.position
                            + Vec3::new(step(&mut scene), Num::ZERO, step(&mut scene));
                        let within = |axis: Num| axis.to_bits().abs() < Num::int(30).to_bits();
                        if within(moved.x) && within(moved.z) {
                            unit.position = moved;
                        }
                    }
                }
                _ => {}
            }
        }
        let seers: Vec<Seer> = slots.iter().flatten().copied().collect();
        let now = Tick::new(tick);
        if tick % 7 == 0 {
            held_reveals.push(Reveal {
                team: Team::new(1),
                position: scene.point(20),
                radius: Num::int(3),
                last: now.after(Ticks::new(9)),
            });
        }
        fog.see(&vision, seers.iter().copied());
        fog.expire(&mut held_reveals, now);
        fog.count_new(&vision, &held_reveals);
        let reveals: Vec<_> = held_reveals
            .since(0)
            .iter()
            .map(|reveal| (reveal.position, reveal.team, reveal.radius))
            .collect();
        assert!(
            held(&fog, &vision) == afresh(&fog, &vision, &seers, &reveals),
            "tick {tick}"
        );
    }
}

#[test]
fn a_cell_stays_seen_while_a_sight_of_its_group_counts_on_it() {
    // Two seers of team 0 at (20.5, 20.5) and (22.5, 20.5) with 1 m of sight: cell (21, 20)'s
    // center lies 1 m from the first and 1 m from the second, so both count on it; (20, 20) is
    // the first's alone and (22, 20) the second's.
    let vision = vision();
    let mut fog = Fog::default();
    fog.rebuild(&vision, &Relations::default());
    let at = |x: i64| Vec3::new(Num::int(x) + Num::HALF, Num::ZERO, Num::int(20) + Num::HALF);
    let seer = |slot: usize, x: i64| Seer {
        slot,
        position: at(x),
        team: Team::new(0),
        range: Num::ONE,
        detects: false,
    };
    let seen = |fog: &Fog| [20, 21, 22].map(|x| fog.sees_point(&vision, at(x), Team::new(0)));
    fog.see(&vision, [seer(0, 20), seer(1, 22)]);
    assert_eq!(seen(&fog), [true, true, true]);
    // The first leaves: its own cell goes dark, the shared one stays seen by the second.
    fog.see(&vision, [seer(1, 22)]);
    assert_eq!(seen(&fog), [false, true, true]);
    // The second leaves too: none is seen, and team 1 never saw any.
    fog.see(&vision, []);
    assert_eq!(seen(&fog), [false, false, false]);
    assert!(fog.maps.is_clear());
}

#[test]
fn a_seer_that_steps_into_a_brush_stops_revealing_the_others() {
    // A seer of team 0 with 20 m of sight, at (6.5, 6.5) outside both brushes, then at
    // (10.5, 8.5) inside the triangle, then back: (0.5, 0.5) lies in the square, (12.5, 9.5) in
    // the triangle; from outside it sees neither brush's cells, from the triangle its own.
    let vision = vision();
    let mut fog = Fog::default();
    fog.rebuild(&vision, &Relations::default());
    let at =
        |x: i64, z: i64| Vec3::new(Num::int(x) + Num::HALF, Num::ZERO, Num::int(z) + Num::HALF);
    let seer = |position| Seer {
        slot: 3,
        position,
        team: Team::new(0),
        range: Num::int(20),
        detects: false,
    };
    let seen = |fog: &Fog| {
        [at(0, 0), at(12, 9), at(20, 20)].map(|point| fog.sees_point(&vision, point, Team::new(0)))
    };
    fog.see(&vision, [seer(at(6, 6))]);
    assert_eq!(seen(&fog), [false, false, true]);
    fog.see(&vision, [seer(at(10, 8))]);
    assert_eq!(seen(&fog), [false, true, true]);
    fog.see(&vision, [seer(at(6, 6))]);
    assert_eq!(seen(&fog), [false, false, true]);
}
