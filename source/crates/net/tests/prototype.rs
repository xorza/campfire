//! The Stage 2 prototype: the sim schedule in Lightyear's `World`, a server and one predicting
//! client over in-process channels, and the server's session log replayed in a bare `World`
//! with the same state hash after every tick.

use std::num::NonZeroU32;

use bevy_app::App;
use bevy_ecs::entity::Entity;
use campfire_capabilities::{
    Action, AttackState, Dead, Destination, Health, MatchEnd, MatchResult, MoveStep, Owner,
    Projectile, Respawn, Team,
};
use campfire_log::internals::capture;
use campfire_math::{Num, PlayerSlot, Vec3};
use campfire_net::{LocalMatch, MatchSetup, PlayerLink, TickHashes, Unpredicted};
use campfire_protocol::{SeedChain, SessionLog};
use campfire_runner::{Runner, Session};
use campfire_sim::{EntityIndex, Position, SimTick, Tick};
use lightyear::prelude::{Predicted, PredictionMetrics, RollbackMode};

use crate::lane::deaths_logged;

const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
/// Frames of match: one tick each.
const MATCH_FRAMES: usize = 120;

fn num(value: i64) -> Num {
    Num::from_int(value).unwrap()
}

fn move_to(x: i64, z: i64) -> Action {
    Action::Move {
        x: num(x),
        z: num(z),
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Hero {
    position: Position,
    destination: Destination,
}

/// The one hero in `app`: the one unit under a player's control.
fn hero_entity(app: &App) -> Entity {
    let world = app.world();
    let mut units = world.resource::<EntityIndex>().iter();
    let (_, entity) = units
        .find(|&(_, entity)| world.entity(entity).contains::<Owner>())
        .unwrap();
    entity
}

fn hero(app: &App) -> Hero {
    let hero = app.world().entity(hero_entity(app));
    Hero {
        position: *hero.get::<Position>().unwrap(),
        destination: *hero.get::<Destination>().unwrap(),
    }
}

#[test]
fn server_and_replay_agree_on_every_tick() {
    // `Check` rolls the client back only on a misprediction; `Always` on every confirmed update,
    // so the client runs the sim again from the server's state many times. A server of 3 frames a
    // tick receives an input in the step's first frame, and runs the tick in one of the three, as
    // the frames it ran alone before shift it: in two of the three, the input arrives in a frame
    // that runs no tick, and must be logged all the same.
    for (rollback, server_frames, shift) in [
        (RollbackMode::Check, 1, 0),
        (RollbackMode::Always, 1, 0),
        (RollbackMode::Check, 3, 0),
        (RollbackMode::Check, 3, 1),
        (RollbackMode::Check, 3, 2),
    ] {
        let case = format!("{rollback:?}, {server_frames} frames, shift {shift}");
        let mut local = LocalMatch::new(MatchSetup::solo(rollback, server_frames, SEED_CHAIN));
        for _ in 0..shift {
            local.server_frame();
        }
        local.start_match();
        // The ticks the server ran while the client learned the match started.
        let started = local.server().world().resource::<SimTick>().start().get();
        for frame in 0..MATCH_FRAMES {
            match frame {
                10 => local.order(0, move_to(0, 5)),
                50 => local.order(0, move_to(-2, 5)),
                _ => {}
            }
            local.step();
        }

        let arrived = Hero {
            position: Position::new(Vec3::new(num(-2), Num::ZERO, num(5))).unwrap(),
            destination: Destination::default(),
        };
        assert_eq!(hero(local.server()), arrived, "{case}");
        assert_eq!(hero(local.client(0)), arrived, "{case}");
        let client_world = local.client(0).world();
        let client_hero = client_world.entity(hero_entity(local.client(0)));
        assert!(client_hero.contains::<Predicted>() && !client_hero.contains::<Unpredicted>());
        let rollbacks = client_world.resource::<PredictionMetrics>().rollbacks;
        // The client stamps each order with the tick it predicts it in, and runs ahead of the
        // server, so the server applies it in that tick: nothing is mispredicted.
        match rollback {
            RollbackMode::Check => assert_eq!(rollbacks, 0),
            _ => assert!(rollbacks > 0, "{case} rolled back {rollbacks} times"),
        }

        let link = local.link(0);
        let server_world = local.server_mut().world_mut();
        assert_eq!(
            server_world
                .entity(link)
                .get::<PlayerLink>()
                .unwrap()
                .refused(),
            0
        );
        server_world.resource_mut::<Session>().reveal_seed();
        let live = server_world.resource::<TickHashes>().get().to_vec();
        let ticks = started + u64::try_from(MATCH_FRAMES).unwrap();
        assert_eq!(u64::try_from(live.len()).unwrap(), ticks);
        let mut file = Vec::new();
        server_world.resource::<Session>().log().encode(&mut file);
        let decoded = SessionLog::decode(&file).unwrap();
        let seed = decoded.revealed_seed().unwrap();
        let mut replay = Runner::new(decoded.rewound(), seed, local.packages()).unwrap();
        for (tick, live) in live.iter().enumerate() {
            replay.run_tick();
            assert_eq!(replay.state_hash(), *live, "{case}, tick {tick}");
        }
        assert_eq!(replay.log().next_tick(), ticks);
    }
}

#[test]
fn a_dead_hero_stays_where_it_died_then_respawns_at_its_spawn_on_the_server_and_its_client() {
    let mut local = LocalMatch::new(MatchSetup::solo(RollbackMode::Check, 1, SEED_CHAIN));
    local.start_match();
    // The hero walks to (4, 0), 4 m from the east tower at (8, 0), which reaches 7.75 m and hits
    // for 150 of its 600: the fourth hit kills it where it stands.
    local.order(0, move_to(4, 0));
    let dead = |app: &App| app.world().entity(hero_entity(app)).contains::<Dead>();
    let rollbacks = |local: &LocalMatch| {
        local
            .client(0)
            .world()
            .resource::<PredictionMetrics>()
            .rollbacks
    };
    // While it attacks, the client holds the tower's target, the hero, and its projectiles in
    // flight, to draw them.
    let hero_id = local.avatar(0);
    let client_sees = |local: &LocalMatch| {
        let world = local.client(0).world();
        let units = || world.resource::<EntityIndex>().iter();
        let aimed = units().any(|(_, entity)| {
            let unit = world.entity(entity);
            unit.get::<AttackState>().and_then(|attack| attack.target()) == Some(hero_id)
                && !unit.contains::<MoveStep>()
        });
        let shot = units().any(|(_, entity)| world.entity(entity).contains::<Projectile>());
        [aimed, shot]
    };
    let mut seen = [false; 2];
    let mut frames = 0;
    let lines = capture(|| {
        while !dead(local.server()) {
            assert!(frames < 400, "the tower kills the hero");
            local.step();
            frames += 1;
            let now = client_sees(&local);
            seen = [seen[0] || now[0], seen[1] || now[1]];
        }
    });
    assert_eq!(seen, [true, true]);
    let died_in = local.server().world().resource::<SimTick>().start().get() - 1;
    // The server logs the avatar's death once: its tick, its team, its player and its killer, an
    // enemy.
    let avatars: Vec<_> = deaths_logged(&lines)
        .into_iter()
        .filter(|death| death.owner.is_some())
        .collect();
    let [death] = avatars[..] else {
        panic!("{avatars:?}");
    };
    assert_eq!(
        (death.tick, death.unit, death.team, death.owner),
        (
            Tick::new(died_in),
            hero_id,
            Some(Team::new(0)),
            Some(PlayerSlot::new(0))
        )
    );
    let world = local.server().world();
    let killer = world.resource::<EntityIndex>().get(death.killer.unwrap());
    assert_eq!(world.get::<Team>(killer.unwrap()), Some(&Team::new(1)));
    // The client learns of the death after the ticks it predicted ahead, and corrects them once.
    for _ in 0..10 {
        local.step();
    }
    assert_eq!(rollbacks(&local), 1);
    // An order after its death moves it on neither end, and needs no correction.
    local.order(0, move_to(-4, 0));
    for _ in 0..40 {
        local.step();
    }
    let at = |x| Hero {
        position: Position::new(Vec3::new(num(x), Num::ZERO, Num::ZERO)).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(local.server()), at(4));
    assert_eq!(hero(local.client(0)), at(4));
    assert!(dead(local.client(0)));
    let health = |app: &App| {
        app.world()
            .get::<Health>(hero_entity(app))
            .unwrap()
            .current()
    };
    assert_eq!(health(local.client(0)), Num::ZERO);
    assert_eq!(rollbacks(&local), 1);

    // The lane mode respawns a hero 5000 ms later: 150 ticks at 30 a second, from the end of the
    // tick it died in. It stands at its team's spawn, (0, 0), with its 600 health, on both ends,
    // and the client, which learned the respawn ahead, needs no correction.
    let back = Respawn {
        at: Tick::new(died_in + 1 + 150),
    };
    let client_hero = hero_entity(local.client(0));
    assert_eq!(
        local.client(0).world().get::<Respawn>(client_hero),
        Some(&back)
    );
    while dead(local.server()) {
        local.step();
    }
    let respawned_in = local.server().world().resource::<SimTick>().start().get() - 1;
    assert_eq!(respawned_in, back.at.get());
    for _ in 0..10 {
        local.step();
    }
    assert_eq!(hero(local.server()), at(0));
    assert_eq!(hero(local.client(0)), at(0));
    assert!(!dead(local.client(0)));
    assert_eq!(health(local.client(0)), num(600));
    assert_eq!(rollbacks(&local), 1);
}

#[test]
fn a_fallen_tower_ends_the_match_on_the_server_and_its_client() {
    let mut local = LocalMatch::new(MatchSetup::solo(RollbackMode::Check, 1, SEED_CHAIN));
    local.start_match();
    // The east tower, of team 1, the one unit of that team that neither walks nor has an owner,
    // falls to one strike: the walker's attack on it ends the match, and the west, team 0, wins.
    let world = local.server().world();
    let (tower, tower_entity) = world
        .resource::<EntityIndex>()
        .iter()
        .find(|&(_, entity)| {
            let unit = world.entity(entity);
            unit.get::<Team>() == Some(&Team::new(1))
                && !unit.contains::<MoveStep>()
                && !unit.contains::<Owner>()
        })
        .unwrap();
    let frail = Health::new(Num::EPSILON).unwrap();
    local
        .server_mut()
        .world_mut()
        .entity_mut(tower_entity)
        .insert(frail);
    local.order(0, Action::Attack { target: tower });
    let mut frames = 0;
    let mut lines = capture(|| {
        while !local.server().world().contains_resource::<MatchEnd>() {
            assert!(frames < 400, "the walker fells the tower");
            local.step();
            frames += 1;
        }
    });
    let end = *local.server().world().resource::<MatchEnd>();
    assert_eq!(end.result(), MatchResult::Won(Team::new(0)));
    for _ in 0..10 {
        local.step();
    }
    assert_eq!(
        local.client(0).world().get_resource::<MatchEnd>(),
        Some(&end)
    );

    // After the end neither end moves the hero, and the server's tick still counts on.
    let still = hero(local.server());
    let tick = local.server().world().resource::<SimTick>().start();
    local.order(0, move_to(-6, 0));
    lines.extend(capture(|| {
        for _ in 0..30 {
            local.step();
        }
    }));
    // The tower's death is logged once, with the walker as killer, and never again after the end,
    // though the record of deaths keeps it.
    let walker = local.avatar(0);
    let towers: Vec<_> = deaths_logged(&lines)
        .into_iter()
        .filter(|death| death.unit == tower)
        .map(|death| (death.tick, death.team, death.owner, death.killer))
        .collect();
    assert_eq!(
        towers,
        [(end.tick(), Some(Team::new(1)), None, Some(walker))]
    );
    assert_eq!(hero(local.server()), still);
    assert_eq!(hero(local.client(0)).position, still.position);
    assert!(local.server().world().resource::<SimTick>().start() > tick);
}
