use std::num::NonZeroI64;

use campfire_sim::MatchRate;
use campfire_units::internals::Spawn;
use campfire_units::{AccelStep, MoveStep, UnitTypeCore};
use campfire_values::TypeScope;

use super::*;

/// 1.5.
const ONE_HALF: Num = Num::from_bits(3 << 23);

/// A tread of Zero Hour's at `at`, facing x, driving to `to`: 3 m a tick, turning 90° a tick,
/// gaining `accel` m a tick each tick and braking 1.5, at whatever rate the match ticks.
fn tread(game: &mut Match, at: (i32, i32, i32), to: Vec3, accel: Num) -> StableId {
    let tread = Units::type_named(game, "tread").unwrap_or_else(|| {
        let tread = Units::load_type(game, TypeScope::Mode, "tread", &UnitTypeCore::default());
        let hz = NonZeroI64::from(game.resource::<MatchRate>().hz()).get();
        let braking = Num::from_bits(3 << 23) * (hz * hz);
        let locomotor = ZeroHourLocomotor::of(LocomotorAppearance::Treads, braking);
        Navigation::load_locomotor(game, tread, locomotor);
        tread
    });
    let body = BodyForm::new(BodyShape::circle(Num::HALF).unwrap(), Layer::FIRST);
    let step = MoveStep::new(Num::int(3)).unwrap();
    let turn = TurnStep::new(Num::int(90)).unwrap();
    let accel = AccelStep::new(accel).unwrap();
    let id = Spawn::of(tread, at)
        .parts((
            body.at(Num::ZERO),
            Navigation::walker(step),
            turn,
            accel,
            ZeroHourDrive::default(),
        ))
        .place(game);
    *game.part_mut::<Destination>(id) = Destination::to(Some(to));
    id
}

#[test]
fn a_tread_drives_to_its_goal_brakes_lands_on_it_and_stops() {
    // From a stop, 1.5 m, then 3 m a tick; 3.5 m short, within its slow-down of
    // (3 / 1.5) × (3 / 1.5) = 4 m, it brakes to 1.5 and stands; braking on, it lands 1.5 m a tick
    // toward its goal, faster than its least landing speed, a cell a second; within its meter of
    // close enough, it stands on its goal and has arrived; then its drive stops.
    let mut game = walk_match();
    let mut client = Match::builder(&[Capability::Navigation])
        .role(Role::Client)
        .build();
    Navigation::walker_type(&mut client);
    // On open ground, so walkers steer and count how stuck they are.
    for game in [&mut game, &mut client] {
        Navigation::load_open_pathing(game, Num::ONE, [-8, -8], [28, 8], [ground(Num::HALF)]);
    }
    let id = tread(&mut game, (0, 0, 0), Vec3::from((20, 0, 0)), ONE_HALF);
    assert_eq!(
        tread(&mut client, (0, 0, 0), Vec3::from((20, 0, 0)), ONE_HALF),
        id
    );
    let stuck_after = Progress::stuck_after(*game.resource::<MatchRate>());
    // Its point each tick, in half meters.
    let halves = [3, 9, 15, 21, 27, 33, 33, 36, 39, 40];
    for (tick, halves) in (1..).zip(halves) {
        game.step();
        client.step();
        let at = game.part::<Position>(id).get();
        assert_eq!(
            at,
            Vec3::new(Num::HALF * halves, Num::ZERO, Num::ZERO),
            "tick {tick}"
        );
        // It comes nearer by half its speed each tick it moves, so it is never stuck as it
        // gathers speed or brakes.
        assert!(
            game.part::<Progress>(id).stuck_ticks() < stuck_after,
            "tick {tick}"
        );
        // A client drives its mover as its server does.
        assert_eq!(client.part::<Position>(id).get(), at, "tick {tick}");
        assert_eq!(
            client.part::<ZeroHourDrive>(id),
            game.part::<ZeroHourDrive>(id)
        );
        if tick == 7 {
            // Mid-brake, its velocity of 1.5 m a tick survives a save and a restore.
            assert_eq!(
                game.part::<ZeroHourDrive>(id).velocity(),
                [Num::from_bits(3 << 23), Num::ZERO]
            );
            let mut restored = walk_match();
            tread(&mut restored, (0, 0, 0), Vec3::from((20, 0, 0)), ONE_HALF);
            Release::restore_into(&game, &mut restored);
        }
    }
    assert_eq!(game.part::<Destination>(id).get(), None);
    game.step();
    assert_eq!(game.part::<Position>(id).get(), Vec3::from((20, 0, 0)));
    assert_eq!(game.part::<ZeroHourDrive>(id).velocity(), [Num::ZERO; 2]);
}

#[test]
fn a_mover_that_gathers_speed_slowly_is_not_stuck() {
    // Gaining a sixteenth of a meter a tick each tick, it comes nearer by 1/16, 2/16, … in its
    // first ticks, far less than half its step of 3 m, but never less than half its speed: it is
    // never stuck, as a walker of its step would be after `Progress::stuck_after` ticks.
    let mut game = walk_match();
    Navigation::load_open_pathing(&mut game, Num::ONE, [-8, -8], [28, 8], [ground(Num::HALF)]);
    let slow = Num::from_bits(1 << 20);
    let id = tread(&mut game, (0, 0, 0), Vec3::from((20, 0, 0)), slow);
    let stuck_after = Progress::stuck_after(*game.resource::<MatchRate>());
    for tick in 1..=2 * stuck_after {
        game.step();
        assert_eq!(game.part::<Progress>(id).stuck_ticks(), 0, "tick {tick}");
    }
    // In n ticks it drove (1 + 2 + … + n) / 16 m, n(n + 1) / 32.
    let n = i64::from(2 * stuck_after);
    let moved = Num::from_bits((n * (n + 1) / 2) << 20);
    assert_eq!(
        game.part::<Position>(id).get(),
        Vec3::new(moved, Num::ZERO, Num::ZERO)
    );
}
