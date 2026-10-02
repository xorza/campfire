//! The Stage 2 prototype: the sim schedule in Lightyear's `World`, a server and one predicting
//! client over in-process channels, and the server's session log replayed in a bare `World`
//! with the same state hash after every tick.

use bevy_app::App;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use campfire_capabilities::internals::{give_modifier, set_relation};
use campfire_capabilities::{
    Action, ActionSlots, Attitude, Bounds, Combat, Dead, Destination, MatchEnd, MatchResult,
    Metric, Modifiers, MoveStep, Owner, PoolId, Pools, Projectile, Relations, Respawn, Stats, Team,
};
use campfire_math::{Num, PlayerSlot, Tick, Ticks, Vec3};
use campfire_net::internals::{End, LocalMatch, MatchSetup};
use campfire_net::{InputChannel, InputMessage, PlayerLink, TickHashes};
use campfire_protocol::{PlayerInput, SessionLog, Signature};
use campfire_runner::internals::HashTrail;
use campfire_runner::{Runner, Session};
use campfire_sim::{EntityIndex, Position, SimTick, Unpredicted};
use lightyear::prelude::{Client, Connected, MessageSender, Predicted, RollbackMode};

/// Frames of match: one tick each.
const MATCH_FRAMES: usize = 120;
const fn move_to(x: i64, z: i64) -> Action {
    Action::Move {
        x: Num::int(x),
        z: Num::int(z),
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

/// The ticks the client runs ahead of the server. An order shows on the server `lead + 1` steps
/// after the client sends it: it is stamped `lead` ticks ahead, and moves the hero in that tick's
/// step. The server's state of a tick the client predicted reaches it as many steps on.
fn lead(local: &LocalMatch) -> u64 {
    local.next_tick(End::Client(0)) - local.next_tick(End::Server)
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
        let mut local = LocalMatch::new(MatchSetup::solo(
            rollback,
            server_frames,
            LocalMatch::SEED_CHAIN,
        ));
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
            position: Position::new(Vec3::new(Num::int(-2), Num::ZERO, Num::int(5))).unwrap(),
            destination: Destination::default(),
        };
        assert_eq!(hero(local.server()), arrived, "{case}");
        assert_eq!(hero(local.client(0)), arrived, "{case}");
        let client_world = local.client(0).world();
        let client_hero = client_world.entity(hero_entity(local.client(0)));
        assert!(
            client_hero.contains::<Predicted>() && !client_hero.contains::<Unpredicted>(),
            "{case}"
        );
        let rollbacks = local.rollbacks(0);
        // The client stamps each order with the tick it predicts it in, and runs ahead of the
        // server, so the server applies it in that tick: nothing is mispredicted, and a client
        // that checks rolls back never. One that always rolls back does so for each server
        // update it takes: one a frame of the match, and the one of its start.
        let expected = match rollback {
            RollbackMode::Check => 0,
            _ => u32::try_from(MATCH_FRAMES + 1).unwrap(),
        };
        assert_eq!(rollbacks, expected, "{case}");

        let link = local.link(0);
        let server_world = local.server_mut().world_mut();
        assert!(
            !server_world
                .entity(link)
                .get::<PlayerLink>()
                .unwrap()
                .refused(),
            "{case}"
        );
        server_world.resource_mut::<Session>().reveal_seed();
        let live = server_world.resource::<TickHashes>().get().to_vec();
        let ticks = started + u64::try_from(MATCH_FRAMES).unwrap();
        assert_eq!(u64::try_from(live.len()).unwrap(), ticks, "{case}");
        let live = HashTrail::of_totals(live);
        let mut file = Vec::new();
        server_world.resource::<Session>().log().encode(&mut file);
        let decoded = SessionLog::decode(&file).unwrap();
        let seed = decoded.revealed_seed().unwrap();
        let mut replay = Runner::new(decoded.rewound(), seed, local.packages()).unwrap();
        let mut replayed = HashTrail::default();
        for _ in live.totals() {
            replay.run_tick();
            replayed.record(replay.world());
        }
        assert_eq!(live.difference(&replayed), None, "{case}");
        assert_eq!(replay.log().next_tick(), Tick::new(ticks), "{case}");
    }
}

#[test]
fn a_burst_of_orders_waits_for_later_stamps_and_a_forged_message_ends_its_link() {
    let mut local = LocalMatch::new(MatchSetup::SOLO);
    local.start_match();
    // Six moves in one frame, past the session's 4 inputs a tick: the client stamps 4 now and 2
    // in the next tick, so the server refuses none, and the last move is where the hero ends.
    for x in 1..=5 {
        local.order(0, move_to(x, 1));
    }
    local.order(0, move_to(-2, 5));
    for _ in 0..=lead(&local) {
        local.step();
    }
    assert_ne!(hero(local.server()).destination, Destination::default());
    let mut frames = 0;
    while hero(local.server()).destination != Destination::default() {
        assert!(frames < 80, "the hero arrives");
        local.step();
        frames += 1;
    }
    let arrived = Hero {
        position: Position::new(Vec3::new(Num::int(-2), Num::ZERO, Num::int(5))).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(local.server()), arrived);
    let link = local.link(0);
    let refused = |local: &LocalMatch| {
        let link = local.server().world().entity(link);
        (
            link.get::<PlayerLink>().unwrap().refused(),
            link.contains::<Connected>(),
        )
    };
    assert_eq!(refused(&local), (false, true));

    // A message whose signature the player's session key did not make: the server refuses it,
    // and ends the link.
    let forged = InputMessage::new(
        [PlayerInput {
            slot: PlayerSlot::new(0),
            stamp: Tick::new(0),
            payload: b"",
        }],
        Signature::from_bytes([0; 64]),
    );
    let world = local.client_mut(0).world_mut();
    let mut senders = world.query_filtered::<&mut MessageSender<InputMessage>, With<Client>>();
    senders
        .single_mut(world)
        .unwrap()
        .send::<InputChannel>(forged);
    // A perfect link carries it in the step it is sent.
    local.step();
    assert_eq!(refused(&local), (true, false));
}

#[test]
fn a_dead_hero_stays_where_it_died_then_respawns_at_its_spawn_on_the_server_and_its_client() {
    let mut local = LocalMatch::new(MatchSetup::SOLO);
    local.start_match();
    // The client predicts on the ground and with the life pool the server's mode installs.
    let (server, client) = (local.server().world(), local.client(0).world());
    assert_eq!(client.resource::<Metric>(), server.resource::<Metric>());
    assert_eq!(client.resource::<Bounds>(), server.resource::<Bounds>());
    assert_eq!(Combat::life(client), Some(PoolId::FIRST));
    assert_eq!(Combat::life(server), Some(PoolId::FIRST));
    // The hero walks to (4, 0), 4 m from the east tower at (8, 0), which reaches 6.25 m from its
    // edge and hits for 150 of its 600: the fourth hit kills it where it stands.
    local.order(0, move_to(4, 0));
    let dead = |app: &App| app.world().entity(hero_entity(app)).contains::<Dead>();
    let rollbacks = |local: &LocalMatch| local.rollbacks(0);
    // While it attacks, the client holds the tower's target, the hero, and its projectiles in
    // flight, to draw them.
    let hero_id = local.avatar(0);
    let client_sees = |local: &LocalMatch| {
        let world = local.client(0).world();
        let units = || world.resource::<EntityIndex>().iter();
        let aimed = units().any(|(_, entity)| {
            let unit = world.entity(entity);
            unit.get::<ActionSlots>()
                .and_then(ActionSlots::attack_target)
                == Some(hero_id)
                && !unit.contains::<MoveStep>()
        });
        let shot = units().any(|(_, entity)| world.entity(entity).contains::<Projectile>());
        [aimed, shot]
    };
    let mut seen = [false; 2];
    let mut frames = 0;
    while !dead(local.server()) {
        assert!(frames < 400, "the tower kills the hero");
        local.step();
        frames += 1;
        let now = client_sees(&local);
        seen = [seen[0] || now[0], seen[1] || now[1]];
    }
    assert_eq!(seen, [true, true]);
    let died_in = local.server().world().resource::<SimTick>().start().get() - 1;
    // The server sent the death in the step it ran it, and the client takes it in the next one:
    // it corrects the ticks it predicted ahead, once.
    local.step();
    assert_eq!(rollbacks(&local), 1);
    // An order after its death moves it on neither end, and needs no correction.
    local.order(0, move_to(-4, 0));
    for _ in 0..=lead(&local) {
        local.step();
    }
    let at = |x, z| Hero {
        position: Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(local.server()), at(4, 0));
    assert_eq!(hero(local.client(0)), at(4, 0));
    assert!(dead(local.client(0)));
    // The lane mode's life pool, `health`, is the first of its pools by name.
    let health = |app: &App| {
        let pools = app.world().get::<Pools>(hero_entity(app)).unwrap();
        pools.current(PoolId::FIRST).unwrap()
    };
    assert_eq!(health(local.client(0)), Num::ZERO);
    assert_eq!(rollbacks(&local), 1);

    // The lane mode respawns a hero 5000 ms later: 150 ticks at 30 a second, from the end of the
    // tick it died in. It stands at its team's spawn, (0, −2), with its 600 health, on both ends,
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
    assert_eq!(hero(local.server()), at(0, -2));
    assert_eq!(hero(local.client(0)), at(0, -2));
    assert!(!dead(local.client(0)));
    assert_eq!(health(local.client(0)), Num::int(600));
    assert_eq!(rollbacks(&local), 1);
}

#[test]
fn a_fallen_tower_ends_the_match_on_the_server_and_its_client() {
    let mut local = LocalMatch::new(MatchSetup::SOLO);
    local.start_match();
    // The east tower, of team 1, the one unit of that team that neither walks nor has an owner,
    // falls to one strike: the walker's attack on it ends the match, and the west, team 0, wins.
    let world = local.server().world();
    let tower = local.tower(Team::new(1));
    let tower_entity = world.resource::<EntityIndex>().get(tower).unwrap();
    let frail = Pools::new([(PoolId::FIRST, Num::EPSILON)]).unwrap();
    local
        .server_mut()
        .world_mut()
        .entity_mut(tower_entity)
        .insert(frail);
    local.order(0, Action::Attack { target: tower });
    // The client starts the walker's attack as the server does: the first windup each shows has
    // the same target, start and cooldowns.
    let slots = |app: &App| app.world().get::<ActionSlots>(hero_entity(app)).cloned();
    let mut windups = [None, None];
    let mut frames = 0;
    while !local.server().world().contains_resource::<MatchEnd>() {
        assert!(frames < 400, "the walker fells the tower");
        local.step();
        frames += 1;
        for (windup, app) in windups.iter_mut().zip([local.server(), local.client(0)]) {
            if windup.is_none() {
                *windup = slots(app).filter(|slots| slots.attacking() == Some(tower));
            }
        }
    }
    let [server, client] = windups;
    assert!(server.is_some());
    assert_eq!(client, server);
    let end = *local.server().world().resource::<MatchEnd>();
    assert_eq!(end.result(), MatchResult::Won(Team::new(0)));
    local.step();
    assert_eq!(
        local.client(0).world().get_resource::<MatchEnd>(),
        Some(&end)
    );

    // After the end neither end moves the hero, and the server's tick still counts on.
    let still = hero(local.server());
    let tick = local.server().world().resource::<SimTick>().start();
    local.order(0, move_to(-6, 0));
    let steps = lead(&local) + 1;
    for _ in 0..steps {
        local.step();
    }
    assert_eq!(hero(local.server()), still);
    assert_eq!(hero(local.client(0)).position, still.position);
    // A step runs one tick of the server's.
    let after = local.server().world().resource::<SimTick>().start();
    assert_eq!(after, tick.after(Ticks::new(steps)));
}

#[test]
fn a_slow_and_a_stun_end_on_the_client_in_the_tick_they_end_on_the_server() {
    let mut local = LocalMatch::new(MatchSetup::SOLO);
    local.start_match();
    // The hero walks off the lane, where nothing meets it: 16.5 m at 0.25 m a tick, so it still
    // walks when both modifiers end, 30 and 15 ticks after the client learns each.
    local.order(0, move_to(-16, -6));
    for _ in 0..=lead(&local) {
        local.step();
    }
    assert_ne!(hero(local.server()).destination, Destination::default());
    let rollbacks = |local: &LocalMatch| local.rollbacks(0);
    let carries = |app: &App| {
        let modifiers = app.world().get::<Modifiers>(hero_entity(app));
        modifiers.is_some_and(|modifiers| *modifiers != Modifiers::default())
    };
    let avatar = local.avatar(0);
    for name in ["slow", "stun"] {
        let world = local.server_mut().world_mut();
        let modifier = Stats::modifier(world, 0, name).unwrap();
        give_modifier(world, avatar, modifier, None, false);
        let mut frames = 0;
        while !carries(local.client(0)) {
            local.step();
            frames += 1;
            assert!(frames < 30, "the client learns the {name}");
        }
        // Learning it may roll the client back; from then on it predicts the hero's walk with
        // the modifier, and without it once it ends, as the server runs it.
        let learned = rollbacks(&local);
        while carries(local.server()) || carries(local.client(0)) {
            local.step();
            frames += 1;
            assert!(frames < 60, "the {name} ends");
        }
        for _ in 0..=lead(&local) {
            local.step();
        }
        assert_eq!(rollbacks(&local), learned, "{name}");
        assert_ne!(hero(local.server()).destination, Destination::default());
    }
}

#[test]
fn the_client_takes_the_relations_a_script_sets() {
    let mut local = LocalMatch::new(MatchSetup::SOLO);
    local.start_match();
    let relations = |app: &App| app.world().resource::<Relations>().clone();
    assert_eq!(relations(local.client(0)), Relations::default());
    let (west, east) = (Team::new(0), Team::new(1));
    set_relation(
        local.server_mut().world_mut(),
        west,
        east,
        Attitude::Neutral,
    );
    // The server sends them in its frame of the next step, and the client takes them in the step
    // after.
    local.step();
    local.step();
    let set = relations(local.server());
    assert_ne!(set, Relations::default());
    assert_eq!(relations(local.client(0)), set);
}
