use super::*;

/// A match of navigation on open half-meter cells over ±16 m, for walkers of a half meter, with
/// boxes that stand by their edges in half meters, `[x0, x1, z0, z1]`.
fn open(boxes: &[[i64; 4]]) -> Match {
    let mut game = walk_match();
    let half = Num::HALF;
    Navigation::load_open_pathing(&mut game, half, [-16, -16], [16, 16], [ground(half)]);
    for &[x0, x1, z0, z1] in boxes {
        let size = [Num::int(x1 - x0) / 2, Num::int(z1 - z0) / 2];
        let at = Vec3::new(Num::int(x0 + x1) / 4, Num::ZERO, Num::int(z0 + z1) / 4);
        Navigation::walker_at(&mut game, at)
            .body(BodyForm::new(
                BodyShape::box_sized(size).unwrap(),
                Layer::FIRST,
            ))
            .place(&mut game);
    }
    game
}

/// A walker of a half meter of team 0 at `at`, in half meters, walking `step` a tick to `to`.
fn walker(game: &mut Match, at: (i64, i64), to: (i64, i64), step: Num) -> StableId {
    Navigation::walker_at(game, halves(at.0, at.1))
        .team(Team::new(0))
        .circle(Num::HALF)
        .walks(step, Some(halves(to.0, to.1)))
        .place(game)
}

/// Each tick's places of `units` over `ticks` ticks.
fn walked(game: &mut Match, units: &[StableId], ticks: usize) -> Vec<Vec<Vec3>> {
    (0..ticks)
        .map(|_| {
            game.step();
            units
                .iter()
                .map(|&unit| game.part::<Position>(unit).get())
                .collect()
        })
        .collect()
}

/// The tick, counted from 1, from which each unit of `track` stood where it ends.
fn arrivals(track: &[Vec<Vec3>]) -> Vec<usize> {
    let last = track.last().unwrap();
    (0..last.len())
        .map(|unit| {
            track
                .iter()
                .rposition(|tick| tick[unit] != last[unit])
                .map_or(0, |tick| tick + 1)
                + 1
        })
        .collect()
}

/// Whether two of the half-meter bodies of a tick of `track` ever overlap by more than a bit.
fn overlap(track: &[Vec<Vec3>]) -> bool {
    let reach = Num::ONE - Num::from_bits(2);
    track.iter().any(|tick| {
        tick.iter().enumerate().any(|(at, &one)| {
            tick[at + 1..].iter().any(|&other| {
                one.ground().distance_order(other.ground(), reach) == std::cmp::Ordering::Less
            })
        })
    })
}

#[test]
fn walkers_abreast_keep_their_lines_and_one_behind_a_slower_walker_brakes_to_it() {
    // Three walkers abreast, 1 m apart, touching, along +x a quarter meter a tick from x = −10 to
    // 10: none is in another's way, so none brakes or moves another, and each keeps its line to
    // the bit and arrives in tick 80, 20 m on (entries-and-blocking.md, A unit outside the cone
    // neither blocks nor pushes).
    let quarter = Num::QUARTER;
    let mut game = open(&[]);
    let units = [-2, 0, 2].map(|z| walker(&mut game, (-20, z), (20, z), quarter));
    let track = walked(&mut game, &units, 90);
    for tick in &track {
        let lines: Vec<Num> = tick.iter().map(|at| at.z).collect();
        assert_eq!(lines, [-Num::ONE, Num::ZERO, Num::ONE]);
    }
    assert_eq!(arrivals(&track), [80; 3]);
    // A walker along z = 0 past a unit of another team that stands 1 m off its line at x = 0,
    // which its body touches as it passes and never enters, walks as it would alone.
    let alone = {
        let mut game = open(&[]);
        let walker = walker(&mut game, (-20, 0), (20, 0), quarter);
        walked(&mut game, &[walker], 90)
    };
    let mut game = open(&[]);
    Navigation::walker_at(&mut game, halves(0, 2))
        .team(Team::new(1))
        .circle(Num::HALF)
        .walks(quarter, None)
        .place(&mut game);
    let passing = walker(&mut game, (-20, 0), (20, 0), quarter);
    assert_eq!(walked(&mut game, &[passing], 90), alone);

    // A walker of an eighth of a meter a tick from x = −6 to 10, and one of a quarter from −8 to
    // 12, 2 m behind it on its line. The faster closes a meter in 8 ticks, to touch the slower,
    // and from then brakes to the slower's advance, an eighth, so it never overlaps it, and the
    // slower walks as it would alone (entries-and-blocking.md, A walker brakes behind a unit that
    // blocks it). The slower stands at its goal from tick 128, 16 m on, as it would alone; the
    // faster stays behind it until then, and the slower, an idle friend on its way, steps aside,
    // to stand from tick 137, and the faster passes and stands from tick 147.
    let eighth = Num::from_bits(ONE / 8);
    let alone = {
        let mut game = open(&[]);
        let slower = walker(&mut game, (-12, 0), (20, 0), eighth);
        walked(&mut game, &[slower], 200)
    };
    let mut game = open(&[]);
    let slower = walker(&mut game, (-12, 0), (20, 0), eighth);
    let faster = walker(&mut game, (-16, 0), (24, 0), quarter);
    let track = walked(&mut game, &[slower, faster], 200);
    let slower_track: Vec<Vec<Vec3>> = track.iter().map(|tick| vec![tick[0]]).collect();
    assert_eq!(slower_track[..128], alone[..128]);
    assert!(!overlap(&track));
    let gaps: Vec<Num> = track[..128]
        .iter()
        .map(|tick| tick[0].x - tick[1].x)
        .collect();
    let closing: Vec<Num> = (1..8)
        .map(|tick| Num::ONE + Num::from_bits(ONE / 8) * (8 - tick))
        .collect();
    assert_eq!(gaps[..7], closing[..]);
    assert!(gaps[7..].iter().all(|&gap| gap == Num::ONE), "{gaps:?}");
    assert_eq!(arrivals(&track), [137, 147]);
}

#[test]
fn columns_that_cross_keep_their_lines_and_the_second_waits_for_the_first() {
    // A column of three along +x from x = −8 m, 1 m apart, and one along +z from z = −11 m, a
    // quarter meter a tick, each walker's goal 10, 9 and 8 m on, front first, so no walker meets
    // one that arrived. The first column crosses the origin from tick 32 to 44; the second's
    // first walker reaches it as the last of the first passes, brakes behind it, as it lies in
    // its way and goes across it, and goes on as it clears (entries-and-blocking.md, A walker
    // brakes behind a unit that blocks it). Each walker keeps its line to the bit.
    let quarter = Num::QUARTER;
    let mut game = open(&[]);
    let mut units = Vec::new();
    for k in 0..3 {
        units.push(walker(
            &mut game,
            (-16 - 2 * k, 0),
            (20 - 2 * k, 0),
            quarter,
        ));
    }
    for k in 0..3 {
        units.push(walker(
            &mut game,
            (0, -22 - 2 * k),
            (0, 20 - 2 * k),
            quarter,
        ));
    }
    let track = walked(&mut game, &units, 140);
    let off = |at: Vec3, unit: usize| if unit < 3 { at.z } else { at.x };
    let farthest: Vec<i64> = (0..6)
        .map(|unit| {
            track
                .iter()
                .map(|tick| off(tick[unit], unit).to_bits().abs())
                .max()
                .unwrap()
        })
        .collect();
    assert_eq!(farthest, [0; 6]);
    assert!(!overlap(&track));
    // Alone, the first column's walkers arrive in tick 72, 18 m on, and the second's in tick 84,
    // 21 m on: the first column arrives as alone, and the second 3, 4 and 4 ticks late, waiting
    // for the first.
    assert_eq!(arrivals(&track), [72, 72, 72, 87, 88, 88]);
}

#[test]
fn a_column_passes_a_gap_one_at_a_time_in_its_order() {
    // Two boxes leave a gap 1.5 m wide along x = 0, from z = −1 to 0.5, which one walker of a
    // half meter passes at a time. Four walkers of a quarter meter a tick come at it from the
    // west, staggered across its mouth, bound for points 4 m past it: each passes in its turn,
    // in the order they stood, 4 ticks apart, as each brakes to touch the one ahead, a meter
    // behind; none steps back on its way, none overlaps another, and each arrives.
    let quarter = Num::QUARTER;
    let mut game = open(&[[-1, 1, 1, 32], [-1, 1, -32, -2]]);
    let starts = [(-8, 1), (-10, -1), (-12, 0), (-14, 2)];
    let goals = [(8, 3), (8, 1), (8, -1), (8, -3)];
    let units: Vec<StableId> = starts
        .iter()
        .zip(goals)
        .map(|(&at, to)| walker(&mut game, at, to, quarter))
        .collect();
    let track = walked(&mut game, &units, 200);
    assert!(!overlap(&track));
    for (unit, to) in goals.into_iter().enumerate() {
        let there = halves(to.0, to.1);
        let arrived = track.iter().position(|tick| tick[unit] == there).unwrap();
        let steps_back = track[..=arrived]
            .windows(2)
            .any(|ticks| ticks[1][unit].x < ticks[0][unit].x);
        assert!(!steps_back, "walker {unit}");
    }
    let through: Vec<usize> = (0..4)
        .map(|unit| {
            track
                .iter()
                .position(|tick| tick[unit].x > Num::ZERO)
                .unwrap()
        })
        .collect();
    assert_eq!(through, [16, 20, 24, 28]);
}

#[test]
fn of_two_walkers_head_on_the_higher_id_steps_to_its_right_and_both_walk_on() {
    // Two walkers of a half meter head on along x, from −4 m and 4 m, a quarter meter a tick:
    // they touch in tick 14, 7 m closed, and in tick 15 each brakes to a stop; the one of the
    // higher id, bound for −x, steps 1 m to its right, +z, and both walk on, the lower id along
    // its line but for a graze of 0.027 m as the other comes back to its line past it
    // (entries-and-blocking.md, A head-on block steps the lower priority aside). Alone, each
    // arrives in tick 32, 8 m on; the lower id arrives in tick 36, and the higher in 38.
    let quarter = Num::QUARTER;
    let mut game = open(&[]);
    let east = walker(&mut game, (-8, 0), (8, 0), quarter);
    let west = walker(&mut game, (8, 0), (-8, 0), quarter);
    let track = walked(&mut game, &[east, west], 60);
    assert!(!overlap(&track));
    let east_off = track
        .iter()
        .map(|tick| tick[0].z.to_bits().abs())
        .max()
        .unwrap();
    let west_off = track.iter().map(|tick| tick[1].z).max().unwrap();
    let stopped = (0..track.len()).find(|&tick| tick > 0 && track[tick] == track[tick - 1]);
    assert_eq!((east_off, west_off, stopped), (451162, Num::ONE, Some(14)));
    assert_eq!(arrivals(&track), [36, 38]);
}

#[test]
fn of_two_walkers_head_on_in_a_lane_the_lower_priority_steps_into_an_alcove_or_both_stop() {
    // A lane 1.5 m wide along x, from z = −1 to 0.5, which two walkers of a half meter cannot
    // pass in side by side, and, above it, an alcove 2 m wide and 2 m deep from x = 0. Two
    // walkers head on along z = −0.5, from −6 m and 6 m, a quarter meter a tick, touch in tick
    // 22 at x = ∓0.5; the higher id, bound for −x, steps 1 m to its right, +z, into the alcove,
    // and both walk on, the other along its line but for a graze of a bit as the first comes back
    // past it, within collision's slack, and arrive in ticks 52 and 59. With no alcove, no side
    // leaves room for either: both stand touching, stuck, until they stop
    // (entries-and-blocking.md, A head-on block steps the lower priority aside;
    // 04-capabilities/navigation.md, A stuck walker plans again, then stops).
    let quarter = Num::QUARTER;
    let run = |alcove: bool| {
        let mut boxes = vec![[-32, 32, -32, -2]];
        if alcove {
            boxes.extend([[-32, 0, 1, 32], [4, 32, 1, 32], [0, 4, 5, 32]]);
        } else {
            boxes.push([-32, 32, 1, 32]);
        }
        let mut game = open(&boxes);
        let east = walker(&mut game, (-12, -1), (12, -1), quarter);
        let west = walker(&mut game, (12, -1), (-12, -1), quarter);
        let mut unparted = 0;
        let track: Vec<Vec<Vec3>> = (0..400)
            .map(|_| {
                game.step();
                unparted += game.resource::<UnpartedPairs>().get();
                [east, west]
                    .iter()
                    .map(|&unit| game.part::<Position>(unit).get())
                    .collect()
            })
            .collect();
        let walking = [east, west].map(|id| game.part::<Destination>(id).get().is_some());
        (track, unparted, walking)
    };
    let (track, unparted, walking) = run(true);
    let east_off = track
        .iter()
        .map(|tick| (tick[0].z + Num::HALF).to_bits().abs())
        .max()
        .unwrap();
    let west_up = track.iter().map(|tick| tick[1].z).max().unwrap();
    assert_eq!((east_off, west_up, unparted), (1, Num::HALF, 0));
    assert_eq!(arrivals(&track), [52, 59]);
    assert_eq!(walking, [false; 2]);
    let (track, _, walking) = run(false);
    let stood = [halves(-1, -1), halves(1, -1)];
    assert_eq!(*track.last().unwrap(), stood);
    assert_eq!(walking, [false; 2]);
}
