//! Bodies that brake, part and block one another, a client's parting, and the layers that pass
//! over each other (04-capabilities/navigation.md).

use super::*;

#[test]
fn bodies_brake_part_and_block_the_way() {
    // Bodies of 0.5 m walking a quarter meter a tick. A from x = 0 and B from x = 3 walk at each
    // other: 3 − 0.5 t apart after tick t, so they touch after tick 4, at 1 and 2, 1 m apart, the
    // sum of their radii, and in tick 5 each brakes to a stop, where they never overlap
    // (04-capabilities/navigation.md, A walker brakes behind a unit that blocks it); the crowd
    // tests show how they pass.
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let mut game = blocking_walk_match();
    Navigation::load_moves(&mut game, Moves::None);
    let a = Navigation::walker_at(&mut game, (0, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((10, 0, 0))))
        .place(&mut game);
    let b = Navigation::walker_at(&mut game, (3, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((-10, 0, 0))))
        .place(&mut game);
    // A tower of radius 1 at (−6, 4), which does not walk, and C walking at it along z from 0: C
    // touches it at z = 2.5, 1.5 m off, and in tick 11, at 2.75, is pushed back all the way.
    let tower = Navigation::walker_at(&mut game, (-6, 0, 4))
        .circle(Num::ONE)
        .place(&mut game);
    let c = Navigation::walker_at(&mut game, (-6, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((-6, 0, 8))))
        .place(&mut game);
    // D walks through a dead body on its way.
    let dead = Navigation::walker_at(&mut game, (20, 0, 0))
        .circle(half)
        .walks(quarter, None)
        .place(&mut game);
    game.insert_parts(dead, Dead);
    let d = Navigation::walker_at(&mut game, (18, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((22, 0, 0))))
        .place(&mut game);
    // A walker at a rooted unit on its way along z = 10, which so stands: the walker touches it
    // in tick 8, at x = 1, and brakes there from tick 9, as a unit that stands blocks it. The
    // rooted unit keeps its place and destination.
    let rooted = Navigation::walker_at(&mut game, (0, 0, 10))
        .circle(half)
        .walks(quarter, Some(Vec3::from((10, 0, 10))))
        .place(&mut game);
    Units::set_blocks(&mut game, rooted, &[Block::Move]);
    let walker = Navigation::walker_at(&mut game, (3, 0, 10))
        .circle(half)
        .walks(quarter, Some(Vec3::from((-10, 0, 10))))
        .place(&mut game);
    game.steps(5);
    assert_eq!(
        [a, b].map(|id| game.part::<Position>(id).get()),
        [Vec3::from((1, 0, 0)), Vec3::from((2, 0, 0))]
    );
    for _ in 5..16 {
        game.step();
    }
    let c_at = Vec3::new(Num::int(-6), Num::ZERO, Num::int(2) + half);
    assert_eq!(
        [c, tower].map(|id| game.part::<Position>(id).get()),
        [c_at, Vec3::from((-6, 0, 4))]
    );
    // D, 16 ticks on at a quarter meter, passed the dead body at 20 to reach 22.
    assert_eq!(
        [d, dead].map(|id| game.part::<Position>(id).get()),
        [Vec3::from((22, 0, 0)), Vec3::from((20, 0, 0))]
    );
    assert_eq!(
        [rooted, walker].map(|id| game.part::<Position>(id).get()),
        [Vec3::from((0, 0, 10)), Vec3::from((1, 0, 10))]
    );
    assert_eq!(
        game.part::<Destination>(rooted).get(),
        Some(Vec3::from((10, 0, 10)))
    );
}

#[test]
fn a_client_parts_its_units_only_from_held_units_that_cannot_walk() {
    // Two own walkers along z = 0 and z = 4, a quarter meter a tick from x = 0 towards x = 4,
    // each with a held unit of the same size, 0.5 m, at x = 2 in its way: on z = 0 one that
    // cannot walk, like a tower; on z = 4 one that can, standing as the server last had it. In
    // tick 4, at 1, the first walker touches the first; in tick 5, at 1.25, it takes the whole
    // overlap and is back at 1. The second walker passes through the second, which may have
    // started walking since, to x = 4. Neither held unit moves.
    let quarter = Num::QUARTER;
    let half = Num::HALF;
    let mut game = walk_match();
    let blocked = Navigation::walker_at(&mut game, (0, 0, 0))
        .circle(half)
        .walks(quarter, Some(Vec3::from((4, 0, 0))))
        .place(&mut game);
    let passing = Navigation::walker_at(&mut game, (0, 0, 4))
        .circle(half)
        .walks(quarter, Some(Vec3::from((4, 0, 4))))
        .place(&mut game);
    let fixed = Navigation::walker_at(&mut game, (2, 0, 0))
        .circle(half)
        .place(&mut game);
    let resting = Navigation::walker_at(&mut game, (2, 0, 4))
        .circle(half)
        .walks(quarter, None)
        .place(&mut game);
    // As on a client: the sim runs on no held unit, and collision names them.
    Unpredicted::register(&mut game);
    for held in [fixed, resting] {
        game.insert_parts(held, Unpredicted);
    }
    game.steps(16);
    let places = [blocked, passing, fixed, resting].map(|id| game.part::<Position>(id).get());
    assert_eq!(
        places,
        [
            Vec3::from((1, 0, 0)),
            Vec3::from((4, 0, 4)),
            Vec3::from((2, 0, 0)),
            Vec3::from((2, 0, 4))
        ]
    );
}

#[test]
fn an_air_unit_passes_over_a_ground_unit_and_a_wall() {
    // Over 1 m cells from (0, 0) to (12, 6), a wall of three towers of 1 m on the ground at
    // x = 6, z = 1, 3 and 5, touching, closes the map to walkers of 0.5 m: every cell center of
    // columns 5 and 6 is 0.71 m from a tower, closer than 1.5. A ground unit of 0.5 m stands at
    // (3, 3).
    let half = Num::HALF;
    let mut game = walk_match();
    let flyer = Walker {
        layer: AIR,
        radius: half,
    };
    Navigation::load_open_pathing(
        &mut game,
        Num::ONE,
        [0, 0],
        [12, 6],
        vec![ground(half), flyer],
    );
    for z in [1, 3, 5] {
        Navigation::walker_at(&mut game, (6, 0, z))
            .circle(Num::ONE)
            .place(&mut game);
    }
    let standing = Navigation::walker_at(&mut game, (3, 0, 3))
        .circle(half)
        .walks(Num::ONE, None)
        .place(&mut game);
    // An air unit of 0.5 m from (1, 3) to (11, 3), a meter a tick, flies straight over both: in
    // tick 2 it is on the ground unit, which does not move, and in tick 10 at its goal.
    let body = BodyForm::new(BodyShape::circle(half).unwrap(), AIR);
    let flying = Navigation::walker_at(&mut game, (1, 0, 3))
        .body(body)
        .walks(Num::ONE, Some(Vec3::from((11, 0, 3))))
        .place(&mut game);
    // A ground unit of 0.5 m from (1, 1) to (11, 1) stops short of the wall, at the cell center
    // nearest its goal that it reaches: (4.5, 0.5) and (4.5, 1.5) tie at √42.5 ≈ 6.52 m, each
    // √2.5 ≈ 1.58 m from the tower at (6, 1), and the lower cell number wins. It walks straight
    // there, √12.5 ≈ 3.54 m, so it arrives in tick 4.
    let walking = Navigation::walker_at(&mut game, (1, 0, 1))
        .circle(half)
        .walks(Num::ONE, Some(Vec3::from((11, 0, 1))))
        .place(&mut game);
    let short = Vec3::new(Num::int(4) + half, Num::ZERO, half);
    let mut flown = Vec::new();
    for _ in 0..12 {
        game.step();
        flown.push(game.part::<Position>(flying).get());
        assert_eq!(game.part::<Position>(standing).get(), Vec3::from((3, 0, 3)));
    }
    let line: Vec<Vec3> = (2..=11)
        .chain([11; 2])
        .map(|x| Vec3::from((x, 0, 3)))
        .collect();
    assert_eq!(flown, line);
    assert_eq!(game.part::<Position>(walking).get(), short);
}
