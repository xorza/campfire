use bevy_ecs::change_detection::Tick;
use bevy_ecs::system::RunSystemOnce;
use campfire_capabilities::Match;
use campfire_core::*;
use campfire_math::Num;
use campfire_mode::Mode;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{Position, StableId};

/// What a reader finds within 2 m of the origin, each unit by its id and its body's bound, and
/// the run of the build it read.
fn near_origin(game: &mut Match) -> (Vec<(StableId, Num)>, Tick) {
    game.run_system_once(|mut near: UnitsNear<'_, '_>| {
        let mut found = Vec::new();
        near.current()
            .visit_near(Position::ORIGIN, Num::int(2), |body| {
                found.push((body.id, body.shape.bound()));
            });
        found.sort_unstable_by_key(|&(id, _)| id);
        (found, near.grid.built.unwrap().tick)
    })
    .unwrap()
}

#[test]
fn readers_share_one_build_until_a_unit_comes_goes_moves_or_changes_its_body() {
    let mut game = Match::client(&[]);
    let unit_type = Units::load_next_type(&mut game, &UnitTypeCore::default());
    Units::give_type(&mut game, unit_type, BodyForm::circle(Num::HALF).unwrap());
    let spawn =
        |game: &mut Match, x: i16| Mode::spawn(game, unit_type, (x, 0, 0), Team::new(0), ());
    let (first, second) = (spawn(&mut game, 0), spawn(&mut game, 10));
    let half = Num::HALF;

    // The first read builds; the second, in another run with nothing changed, reads that build.
    let (found, built) = near_origin(&mut game);
    assert_eq!(found, [(first, half)]);
    assert_eq!(near_origin(&mut game), (vec![(first, half)], built));

    // A unit that moves near: a build of its new place.
    *game.part_mut::<Position>(second) = Position::from((1, 0, 0));
    let (found, moved) = near_origin(&mut game);
    assert_eq!(found, [(first, half), (second, half)]);
    assert_ne!(moved, built);

    // A unit that dies stays, for each reader checks life as it visits: no build.
    game.insert_parts(first, Dead);
    assert_eq!(near_origin(&mut game), (found, moved));

    // A unit that comes, then one that goes.
    let third = spawn(&mut game, -1);
    let (found, came) = near_origin(&mut game);
    assert_eq!(found, [(first, half), (second, half), (third, half)]);
    assert_ne!(came, moved);
    let gone = game.unit_entity(second);
    game.despawn(gone);
    let (found, went) = near_origin(&mut game);
    assert_eq!(found, [(first, half), (third, half)]);
    assert_ne!(went, came);

    // A unit that loses its body, which no change tick shows: a point, of no bound.
    let unbodied = game.unit_entity(third);
    game.entity_mut(unbodied).remove::<Body>();
    let (found, lost) = near_origin(&mut game);
    assert_eq!(found, [(first, half), (third, Num::ZERO)]);
    assert_ne!(lost, went);
}
