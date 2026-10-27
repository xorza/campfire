use std::num::NonZeroI64;

use campfire_sim::MatchRate;
use campfire_units::internals::Spawn;
use campfire_units::{CellSplit, Ground, HeightGrid, MapSpace, MoveStep, StatusTags, UnitTypeCore};
use campfire_values::TypeScope;

use super::*;

/// A flier of a meter's step a tick, of a flight at 8 m that climbs a meter a tick, airborne past
/// 3 m, on the air layer, at `at`, walking to `to`.
fn flier(game: &mut Match, at: (i32, i32, i32), to: Option<Vec3>) -> StableId {
    let flier = Units::type_named(game, "flier").unwrap_or_else(|| {
        let flier = Units::load_type(game, TypeScope::Mode, "flier", &UnitTypeCore::default());
        let hz = NonZeroI64::from(game.resource::<MatchRate>().hz()).get();
        let flight = FlightData::new(Num::int(8), Num::int(hz), Num::int(3)).unwrap();
        Navigation::load_flight(game, flier, flight);
        flier
    });
    let body = BodyForm::new(BodyShape::circle(Num::HALF).unwrap(), AIR);
    let step = MoveStep::new(Num::ONE).unwrap();
    let id = Spawn::of(flier, at)
        .parts((body.at(Num::ZERO), Navigation::walker(step)))
        .place(game);
    *game.part_mut::<Destination>(id) = Destination::to(to);
    id
}

/// The flier `id`'s place and whether it is an air target.
fn flown(game: &Match, id: StableId) -> (Vec3, bool) {
    let status = game.part::<StatusTags>(id).get();
    let tag = Navigation::AIR_TARGET.tag();
    (game.part::<Position>(id).get(), status.contains(tag))
}

#[test]
fn a_flier_climbs_to_its_height_at_its_rate_walks_in_the_plane_and_eases_over_the_ground() {
    // A ground of samples 10 m apart along x: flat at 0 m to x = 10, rising 2 m a meter to 20 m
    // at x = 20, flat to x = 30, falling to 0 m at x = 40, flat past it.
    let mut game = walk_match();
    let samples = [0, 0, 20, 20, 0];
    let rows = [samples, samples].concat();
    let grid = HeightGrid::new([Num::ZERO; 2], Num::int(10), Num::ONE, 5, rows).unwrap();
    let ground = Ground::new(grid, CellSplit::ZeroTwo, None).unwrap();
    MapBook::amend(&mut game, |_, map| map.space = MapSpace::Planar(ground));
    // From the origin to (45, 0, 0), a meter a tick in the plane, so it stands at x = t after
    // tick t, whatever its height, until it arrives in tick 45.
    let id = flier(&mut game, (0, 0, 0), Some(Vec3::from((45, 0, 0))));
    let mut ticks = vec![(Vec3::ZERO, false)];
    for _ in 0..51 {
        game.step();
        ticks.push(flown(&game, id));
    }
    let at = |x: i32, height: i32| Vec3::from((x, height, 0));
    for (tick, place, target) in [
        // It climbs a meter a tick from the ground: an air target from 4 m up, past its 3 m.
        (3, at(3, 3), false),
        (4, at(4, 4), true),
        (8, at(8, 8), true),
        (10, at(10, 8), true),
        // The ground rises 2 m a tick under it as it climbs 1: 8 m in the world after tick 10,
        // 13 after tick 15 over ground of 10, 3 m above it.
        (14, at(14, 4), true),
        (15, at(15, 3), false),
        // Over ground of 16 m at tick 18 it stands on it, and climbs no faster than the ground
        // rises: on it at 18 and 20 m too.
        (18, at(18, 0), false),
        (20, at(20, 0), false),
        // Over the flat it climbs back, to 28 m in the world after tick 28.
        (28, at(28, 8), true),
        // The ground falls 2 m a tick as it sinks 1: 27 m after tick 31 over 18 m, 9 above.
        (31, at(31, 9), true),
        (40, at(40, 18), true),
        // Arrived at tick 45, it sinks on to 8 m above the ground at tick 50, and holds there.
        (45, at(45, 13), true),
        (50, at(45, 8), true),
        (51, at(45, 8), true),
    ] {
        assert_eq!(ticks[tick], (place, target), "tick {tick}");
    }
    assert_eq!(game.part::<Altitude>(id).get(), Num::int(8));
}

/// A winged flier of a meter's step a tick, turning 90° a tick, of a flight at 8 m that climbs a
/// meter a tick, airborne past 3 m, of wings that circle 4 m round the place they hold at half a
/// meter a tick, on the air layer, at `at`, walking to `to`.
fn winged(game: &mut Match, at: (i32, i32, i32), to: Option<Vec3>) -> StableId {
    let winged = Units::type_named(game, "winged").unwrap_or_else(|| {
        let winged = Units::load_type(game, TypeScope::Mode, "winged", &UnitTypeCore::default());
        let hz = NonZeroI64::from(game.resource::<MatchRate>().hz()).get();
        let wings = WingsData::new(Num::int(hz) / Num::int(2), Num::int(4)).unwrap();
        let flight = FlightData::new(Num::int(8), Num::int(hz), Num::int(3)).unwrap();
        Navigation::load_flight(game, winged, flight.winged(wings));
        winged
    });
    let body = BodyForm::new(BodyShape::circle(Num::HALF).unwrap(), AIR);
    let step = MoveStep::new(Num::ONE).unwrap();
    let turn = TurnStep::new(Num::int(15)).unwrap();
    let id = Spawn::of(winged, at)
        .parts((body.at(Num::ZERO), Navigation::walker(step), turn))
        .place(game);
    *game.part_mut::<Destination>(id) = Destination::to(to);
    id
}

#[test]
fn a_winged_flier_never_stands_in_the_air_and_circles_the_place_it_holds() {
    // From the origin, facing x, to (10, 0, 0) ahead: on the ground it walks as a walker, a meter
    // a tick, and climbs a meter a tick, an air target after tick 4, so on its wings from tick 5;
    // it reaches its goal within its circling radius of 4 m at x = 6 in tick 6, holds that place
    // and circles it at half a meter a tick (04-capabilities/navigation.md, A jet flies on wings).
    let mut game = walk_match();
    let id = winged(&mut game, (0, 0, 0), Some(Vec3::from((10, 0, 0))));
    let mut places = vec![Vec3::ZERO];
    let mut facings = vec![Num::ZERO];
    for tick in 1..=52 {
        // Sent back to (−10, 0, 0) in tick 41, it flies at its meter a tick again.
        if tick == 41 {
            *game.part_mut::<Destination>(id) = Destination::to(Some(Vec3::from((-10, 0, 0))));
        }
        game.step();
        places.push(game.part::<Position>(id).get());
        facings.push(game.part::<Facing>(id).degrees());
        if tick <= 6 {
            let meters = i32::try_from(tick).unwrap();
            assert_eq!(places[tick], Vec3::from((meters, meters, 0)), "tick {tick}");
        }
    }
    assert_eq!(
        game.part::<Destination>(id).get(),
        Some(Vec3::from((-10, 0, 0)))
    );
    let held = game.part::<HeldPlace>(id).get();
    assert_eq!(held, Vec3::from((10, 0, 0)));
    // The circle's point 157.5° round from its bearing to the place lies off its facing, so it
    // turns its whole 15° a tick toward it for five ticks: 15°, 30°, 45°, 60°, 75°.
    for (tick, facing) in (7..=11).zip([15, 30, 45, 60, 75]) {
        assert_eq!(facings[tick], Num::int(facing), "tick {tick}");
    }
    // Across the place from where it arrived, 3.8 m off on its circle, in tick 30.
    let across = |text: &str| text.parse::<Num>().unwrap();
    assert_eq!(
        places[30],
        Vec3::new(
            across("13.7954905033111572265625"),
            Num::int(8),
            across("0.0683486461639404296875")
        )
    );
    // It never stands in the air: each tick it moves its least speed's half meter round the
    // place, then its meter toward its new goal, and turns at most its 15°; the step's sine and
    // cosine round each to 2⁻²⁴, so its length lies within 2⁻²⁰ of the speed.
    let rounding = Num::from_bits(16);
    for tick in 7..=52 {
        let speed = if tick <= 40 { Num::HALF } else { Num::ONE };
        let moved = places[tick - 1].ground_offset(places[tick]).length();
        assert!(
            speed - rounding <= moved && moved <= speed + rounding,
            "tick {tick}: {moved}"
        );
        let turned = Facing::of(facings[tick - 1]).apart(Facing::of(facings[tick]));
        assert!(turned <= Num::int(15), "tick {tick}: {turned}");
    }
}
