//! The prototype of milestone 1: the sim schedule in Lightyear's `World`, a server and one predicting
//! client over in-process channels, and the server's session log replayed in a bare `World`
//! with the same state hash after every tick.

use bevy_app::App;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::With;
use bevy_ecs::world::EntityRef;
use campfire_capabilities::{
    Action, ActionSlots, Bounds, CombatWorld, DashTo, Dead, Destination, ForcedMove, Life,
    MatchResult, Metric, ModeState, ModeWorld, Modifiers, MoveStep, Order, Owner, PlayerResources,
    PlayersWorld, PoolId, Projectile, Relation, Relations, ResourceId, Respawn, StateValue,
    SyncAll, SyncNone, Team, UnitsWorld,
};
use campfire_capabilities::{Stats, Units};
use campfire_common::{MapName, PlayerSlot, Tick, Ticks};
use campfire_math::{Num, Vec3};
use campfire_net::internals::{End, InProcessMatch, LinkModel, MatchSetup};
use campfire_net::{
    InputChannel, InputMessage, InputMessageRefused, JoinState, LinkLost, PendingOrders,
    PlayerLink, TickHashes,
};
use campfire_package::{ModePackages, PackageDir};
use campfire_protocol::{InputError, PlayerInput, SessionLog, Signature};
use campfire_quic::LINK_MIN_MTU;
use campfire_runner::internals::HashTrail;
use campfire_runner::{Session, SessionWorld};
use campfire_sim::{EntityIndex, Position, SimTick, SimWorld, StableId, Unpredicted};
use lightyear::prelude::{
    Client, Connected, Link, MessageSender, Predicted, PredictionManager, PredictionTarget,
    Replicate, RollbackMode,
};

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
    pos: Position,
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
        pos: *hero.get::<Position>().unwrap(),
        destination: *hero.get::<Destination>().unwrap(),
    }
}

/// The ticks the client runs ahead of the server. An order shows on the server `lead + 1` steps
/// after the client sends it: it is stamped `lead` ticks ahead, and moves the hero in that tick's
/// step. The server's state of a tick the client predicted reaches it as many steps on.
fn lead(local: &InProcessMatch) -> u64 {
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
        let mut local = InProcessMatch::new(MatchSetup::solo(
            rollback,
            server_frames,
            InProcessMatch::SEED_CHAIN,
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
            pos: Position::new(Vec3::new(Num::int(-2), Num::ZERO, Num::int(5))).unwrap(),
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
        let seed = decoded.revealed_seeds().unwrap();
        let mut replay = Session::start_match(decoded.rewound(), seed, local.packages()).unwrap();
        let mut replayed = HashTrail::default();
        for _ in live.totals() {
            replay.run_tick_alone();
            replayed.record(&replay);
        }
        assert_eq!(live.difference(&replayed), None, "{case}");
        assert_eq!(
            replay.session().log().next_tick(),
            Tick::new(ticks),
            "{case}"
        );
    }
}

#[test]
fn a_burst_of_orders_waits_for_later_stamps_and_a_forged_message_ends_its_link() {
    let mut local = InProcessMatch::new(MatchSetup::SOLO);
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
        pos: Position::new(Vec3::new(Num::int(-2), Num::ZERO, Num::int(5))).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(local.server()), arrived);
    let link = local.link(0);
    // Both ends of a link take a QUIC link's minimum MTU from their spawn, so their fragments
    // agree whatever IO links them (design 15, D5).
    let server_link = local.server().world().get::<Link>(link).unwrap();
    assert_eq!(server_link.min_mtu(), LINK_MIN_MTU);
    let client_links: Vec<usize> = local
        .client(0)
        .world()
        .iter_entities()
        .filter(EntityRef::contains::<Client>)
        .map(|entity| entity.get::<Link>().unwrap().min_mtu())
        .collect();
    assert_eq!(client_links, [LINK_MIN_MTU]);
    let refused = |local: &InProcessMatch| {
        let link = local.server().world().entity(link);
        (
            link.get::<PlayerLink>().map(|player| player.refused()),
            link.contains::<Connected>(),
        )
    };
    assert_eq!(refused(&local), (Some(false), true));

    // A message whose signature the player's session key did not make, stamped for the next tick
    // so that only its signature is wrong: the server refuses it, and ends the link.
    let next_tick = Tick::new(local.next_tick(End::Server));
    let forged = InputMessage::new(
        [PlayerInput {
            slot: PlayerSlot::new(0),
            stamp: next_tick,
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
    // A perfect link carries it in the step it is sent. The ended link carries the slot no more,
    // and its player's grace period runs.
    local.step();
    assert_eq!(refused(&local), (None, false));
    assert_eq!(
        local.log().take::<InputMessageRefused>(),
        [InputMessageRefused {
            slot: PlayerSlot::new(0),
            next_tick,
            error: InputError::BadSignature.to_string(),
        }]
    );
}

#[test]
fn a_dead_hero_stays_where_it_died_then_respawns_at_its_spawn_on_the_server_and_its_client() {
    let mut local = InProcessMatch::new(MatchSetup::SOLO);
    local.start_match();
    // The client predicts on the ground and with the life pool the server's mode installs.
    let (server, client) = (local.server().world(), local.client(0).world());
    assert_eq!(client.resource::<Metric>(), server.resource::<Metric>());
    assert_eq!(client.resource::<Bounds>(), server.resource::<Bounds>());
    assert_eq!(client.life_pool(), Some(PoolId::FIRST));
    assert_eq!(server.life_pool(), Some(PoolId::FIRST));
    // The hero walks to (4, 0), 4 m from the east tower at (8, 0), which reaches 6.25 m from its
    // edge and hits for 150 of its 600: the fourth hit kills it where it stands.
    local.order(0, move_to(4, 0));
    let dead = |app: &App| app.world().entity(hero_entity(app)).contains::<Dead>();
    let rollbacks = |local: &InProcessMatch| local.rollbacks(0);
    // While it attacks, the client holds the tower's target, the hero, and its projectiles in
    // flight, to draw them.
    let hero_id = local.avatar(0);
    let client_sees = |local: &InProcessMatch| {
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
        pos: Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap(),
        destination: Destination::default(),
    };
    assert_eq!(hero(local.server()), at(4, 0));
    assert_eq!(hero(local.client(0)), at(4, 0));
    assert!(dead(local.client(0)));
    let health = |app: &App| app.world().get::<Life>(hero_entity(app)).unwrap().current();
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
fn a_fallen_tower_ends_the_match_on_the_server_its_client_and_a_spectator() {
    let mut local = InProcessMatch::new(MatchSetup {
        max_spectators: 1,
        ..MatchSetup::SOLO
    });
    local.start_match();
    let spectator = local.add_spectator();
    for _ in 0..300 {
        if local
            .client(spectator)
            .world()
            .resource::<JoinState>()
            .watching()
            .is_some()
        {
            break;
        }
        local.step();
    }
    // The east tower, of team 1, the one unit of that team that neither walks nor has an owner,
    // falls to one strike: the walker's attack on it ends the match, and the west, team 0, wins.
    let world = local.server().world();
    let tower = local.tower(Team::new(1));
    let tower_entity = world.entity_of(tower).unwrap();
    let frail = Life::full(Num::EPSILON);
    local
        .server_mut()
        .world_mut()
        .entity_mut(tower_entity)
        .insert(frail);
    // The tower, at (8, -3), stands out of the walker's sight at (0, -2): an attack on it does
    // nothing, on the server and on the client, which does not hold it.
    let slots = |app: &App| app.world().get::<ActionSlots>(hero_entity(app)).cloned();
    let attacking = |app: &App| slots(app).and_then(|slots| slots.attacking());
    let start = hero(local.server());
    local.order(0, Action::Attack { target: tower });
    for _ in 0..lead(&local) + 2 {
        local.step();
        assert_eq!(attacking(local.server()), None);
        assert_eq!(attacking(local.client(0)), None);
    }
    assert_eq!(hero(local.server()), start);
    // Once the walker walks east until its client holds the tower, the same attack takes effect.
    local.order(0, move_to(4, -2));
    let held = |local: &InProcessMatch| {
        let index = local.client(0).world().resource::<EntityIndex>();
        index.get(tower).is_some()
    };
    let mut frames = 0;
    while !held(&local) {
        assert!(frames < 200, "the walker comes to see the tower");
        local.step();
        frames += 1;
    }
    local.order(0, Action::Attack { target: tower });
    // The client starts the walker's attack as the server does: the first windup each shows has
    // the same target, start and cooldowns.
    let mut windups = [None, None];
    let mut frames = 0;
    while local.server().world().match_end().is_none() {
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
    let end = local.server().world().match_end().unwrap();
    assert_eq!(end.result(), MatchResult::Won(Team::new(0)));
    local.step();
    assert_eq!(local.client(0).world().match_end(), Some(end));
    // A spectator holds the match's entity, and its end with it (design 14, D7).
    assert_eq!(local.client(spectator).world().match_end(), Some(end));

    // After the end neither end moves the hero, and the server's tick still counts on.
    let still = hero(local.server());
    let tick = local.server().world().resource::<SimTick>().start();
    local.order(0, move_to(-6, 0));
    let steps = lead(&local) + 1;
    for _ in 0..steps {
        local.step();
    }
    assert_eq!(hero(local.server()), still);
    assert_eq!(hero(local.client(0)).pos, still.pos);
    // A step runs one tick of the server's.
    let after = local.server().world().resource::<SimTick>().start();
    assert_eq!(after, tick.after(Ticks::new(steps)));

    // A second login of the player, which sits after the end, learns it as it sits.
    let second = local.add_client(0);
    let ended = |local: &InProcessMatch| local.client(second).world().match_end();
    for _ in 0..300 {
        if ended(&local).is_some() {
            break;
        }
        local.step();
    }
    assert_eq!(ended(&local), Some(end));
    // The first client loses its link once told of the newer login.
    for _ in 0..5 {
        local.step();
    }
    assert_eq!(local.log().take::<LinkLost>().len(), 1);
}

#[test]
fn a_slow_and_a_stun_end_on_the_client_in_the_tick_they_end_on_the_server() {
    let mut local = InProcessMatch::new(MatchSetup::SOLO);
    local.start_match();
    // The hero walks off the lane, where nothing meets it: 16.5 m at 0.25 m a tick, so it still
    // walks when both modifiers end, 30 and 15 ticks after the client learns each.
    local.order(0, move_to(-16, -6));
    for _ in 0..=lead(&local) {
        local.step();
    }
    assert_ne!(hero(local.server()).destination, Destination::default());
    let rollbacks = |local: &InProcessMatch| local.rollbacks(0);
    let carries = |app: &App| {
        let modifiers = app.world().get::<Modifiers>(hero_entity(app));
        modifiers.is_some_and(|modifiers| *modifiers != Modifiers::default())
    };
    let avatar = local.avatar(0);
    for name in ["slow", "stun"] {
        let world = local.server_mut().world_mut();
        let modifier = Stats::modifier(world, 0, name).unwrap();
        Stats::give_modifier(world, avatar, modifier, None, false);
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
fn a_client_continues_a_dash_it_learns_of_as_the_server_runs_it() {
    let mut local = InProcessMatch::new(MatchSetup::SOLO);
    local.start_match();
    // The hero walks off the lane, where nothing meets it, as in the stun's test. The server
    // dashes it 6 m along −x, a fifth of a meter a tick, 30 ticks: it learns the dash some
    // ticks in, and predicts the rest of it, and its walk again once it ends, as the server
    // runs them.
    local.order(0, move_to(-16, -6));
    for _ in 0..=lead(&local) {
        local.step();
    }
    let rollbacks = |local: &InProcessMatch| local.rollbacks(0);
    let dashing = |app: &App| app.world().get::<ForcedMove>(hero_entity(app)).is_some();
    let entity = hero_entity(local.server());
    let world = local.server_mut().world_mut();
    let pos = world.get::<Position>(entity).unwrap().get();
    let to = Position::new(Vec3::new(pos.x - Num::int(6), pos.y, pos.z)).unwrap();
    let step = Num::ONE.checked_div_int(5).unwrap();
    world.entity_mut(entity).insert(ForcedMove::Dash {
        to: DashTo::Point(to),
        step,
        delivers: None,
    });
    let mut frames = 0;
    while !dashing(local.client(0)) {
        local.step();
        frames += 1;
        assert!(frames < 30, "the client learns the dash");
    }
    let learned = rollbacks(&local);
    while dashing(local.server()) || dashing(local.client(0)) {
        local.step();
        frames += 1;
        assert!(frames < 60, "the dash ends");
    }
    for _ in 0..=lead(&local) {
        local.step();
    }
    assert_eq!(rollbacks(&local), learned);
    assert_ne!(hero(local.server()).destination, Destination::default());
}

#[test]
fn the_client_takes_the_relations_a_script_sets() {
    let mut local = InProcessMatch::new(MatchSetup::SOLO);
    local.start_match();
    // The client holds the relations from its first replication, on the match's entity.
    let relations = |app: &App| app.world().held_relations().cloned();
    while relations(local.client(0)).is_none() {
        local.step();
    }
    assert_eq!(relations(local.client(0)), Some(Relations::default()));
    let (west, east) = (Team::new(0), Team::new(1));
    Units::set_relation(
        local.server_mut().world_mut(),
        west,
        east,
        Relation::Neutral,
    );
    // The server sends them in its frame of the next step, and the client takes them in the step
    // after.
    local.step();
    local.step();
    let set = relations(local.server());
    assert_ne!(set, Some(Relations::default()));
    assert_eq!(relations(local.client(0)), set);
}

#[test]
fn each_link_holds_its_own_players_resources_and_every_link_the_match_and_its_players() {
    // The proving mode for two players, with a server frame a tick: each holds its income of 100
    // gold from the start, and 100 more every 5 s, 100 ticks at its 20 Hz; its state's `joined`
    // goes to every client, and `left` to none.
    let map = MapName::new("proving").unwrap();
    let packages = ModePackages::from_dir(&PackageDir::workspace("test/modes/proving"), &map);
    let packages = packages.unwrap();
    let gold = ResourceId::named(&packages.data().resources, "gold").unwrap();
    let mut setup = MatchSetup::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN);
    setup.players = 2;
    let mut local = InProcessMatch::of_mode(setup, packages);
    local.start_match();
    // A player's gold as `app` holds it: `None` for a player's entity it lacks, `Some(None)` for
    // one whose resources it lacks.
    let gold_of = |app: &App, slot| {
        let world = app.world();
        let entity = world.player_entity(PlayerSlot::new(slot))?;
        Some(
            world
                .get::<PlayerResources>(entity)
                .map(|held| held.amount(gold)),
        )
    };
    let both_held = |local: &InProcessMatch| {
        (0..2).all(|client| (0..2).all(|slot| gold_of(local.client(client), slot).is_some()))
    };
    while !both_held(&local) {
        local.step();
    }
    // Past the first income, every client holds both players' entities, its own player's gold
    // as the server holds it, and never the other's.
    let mut frames = 0;
    while gold_of(local.server(), 0) != Some(Some(200)) {
        assert!(frames < 200, "the income comes");
        local.step();
        frames += 1;
        for (client, other) in [(0, 1), (1, 0)] {
            assert_eq!(gold_of(local.client(client), other), Some(None));
        }
    }
    local.step();
    local.step();
    for client in 0..2 {
        let own = u32::try_from(client).unwrap();
        assert_eq!(gold_of(local.client(client), own), Some(Some(200)));
        assert_eq!(gold_of(local.client(client), 1 - own), Some(None));
    }
    // The mode's state that syncs to all reaches each client, and the rest none.
    for app in [local.server(), local.client(0), local.client(1)] {
        let world = app.world();
        let entity = world.match_entity().unwrap();
        let all = world
            .get::<ModeState<SyncAll>>(entity)
            .map(ModeState::values);
        assert_eq!(all, Some(&[StateValue::Int(-1)][..]));
    }
    let none = |app: &App| {
        let world = app.world();
        world
            .get::<ModeState<SyncNone>>(world.match_entity().unwrap())
            .is_some()
    };
    assert!(none(local.server()) && !none(local.client(0)) && !none(local.client(1)));
}

#[test]
fn each_players_client_rolls_back_as_the_setup_gives_for_that_player() {
    let mut setup = MatchSetup::duo(LinkModel::PERFECT, InProcessMatch::SEED_CHAIN);
    setup.rollbacks[1] = RollbackMode::Always;
    let local = InProcessMatch::new(setup);
    let mode = |client: usize| {
        let world = local.client(client).world();
        world.resource::<PredictionManager>().rollback_policy.state
    };
    // `RollbackMode` compares by no `PartialEq`, so by its pattern.
    let modes = [mode(0), mode(1)];
    assert!(
        matches!(modes, [RollbackMode::Check, RollbackMode::Always]),
        "{modes:?}"
    );
}

/// How a client holds a unit: predicted, as the server sends it, or with no marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Held {
    Predicted,
    Unpredicted,
    Unmarked,
}

/// A match of two players whose first player's hero changes owner, and the hero's stable id.
#[derive(Debug)]
struct Handoff {
    local: InProcessMatch,
    hero: StableId,
}

impl Handoff {
    /// Frames the scenario waits for a change to reach the clients, at most.
    const FRAMES: usize = 60;

    fn entity_in(&self, app: &App) -> Entity {
        app.world().entity_of(self.hero).unwrap()
    }

    fn held(&self, client: usize) -> Held {
        let app = self.local.client(client);
        let unit = app.world().entity(self.entity_in(app));
        match (unit.contains::<Predicted>(), unit.contains::<Unpredicted>()) {
            (true, false) => Held::Predicted,
            (false, true) => Held::Unpredicted,
            (false, false) => Held::Unmarked,
            (true, true) => panic!("a client holds a unit predicted and unpredicted"),
        }
    }

    fn at(&self, end: End) -> Position {
        let app = self.local.app(end);
        *app.world().get::<Position>(self.entity_in(app)).unwrap()
    }

    fn slot(&self, client: usize) -> PlayerSlot {
        let world = self.local.server().world();
        world
            .get::<PlayerLink>(self.local.link(client))
            .unwrap()
            .slot()
    }

    /// Gives the hero to `owner` on the server, or takes its owner.
    fn set_owner(&mut self, owner: Option<PlayerSlot>) {
        let entity = self.entity_in(self.local.server());
        let mut unit = self.local.server_mut().world_mut().entity_mut(entity);
        match owner {
            Some(slot) => unit.insert(Owner::new(slot)),
            None => unit.remove::<Owner>(),
        };
    }

    /// Steps every end until `done`.
    fn step_until(&mut self, done: impl Fn(&Handoff) -> bool) {
        for _ in 0..Handoff::FRAMES {
            if done(self) {
                return;
            }
            self.local.step();
        }
        assert!(done(self), "the change reaches the clients");
    }

    /// Steps the server and the first client alone until `done`.
    fn step_first_until(&mut self, done: impl Fn(&Handoff) -> bool) {
        for _ in 0..Handoff::FRAMES {
            if done(self) {
                return;
            }
            self.local.client_frame(0);
            for _ in 0..self.local.setup().server_frames {
                self.local.server_frame();
            }
        }
        assert!(done(self), "the change reaches the first client");
    }
}

#[test]
fn a_unit_whose_owner_changes_mid_match_is_predicted_by_its_new_owners_client_alone() {
    let mut local = InProcessMatch::new(MatchSetup::duo(
        LinkModel::PERFECT,
        InProcessMatch::SEED_CHAIN,
    ));
    local.start_match();
    let hero = local.avatar(0);
    let mut game = Handoff { local, hero };
    let held_in = |app: &App| app.world().entity_of(hero).is_some();
    game.step_until(|game| held_in(game.local.client(0)) && held_in(game.local.client(1)));
    assert_eq!(
        (game.held(0), game.held(1)),
        (Held::Predicted, Held::Unpredicted)
    );
    let (first, second) = (game.slot(0), game.slot(1));

    // The hero walks under its first owner, and the server gives it to the second player's slot
    // on its way, as soon as it left its spawn: the second client takes it predicted, with its
    // state as the server last sent it, and rolls back once to bring it to its own tick; the
    // first takes it unpredicted. The second player's hero stands at its spawn, (0, 2), and sees
    // 6 m: the first hero walks within 5 m of it, so the second client, which receives what its
    // team sees, holds it throughout.
    let spawn = game.at(End::Server);
    game.local.order(0, move_to(-4, -1));
    game.step_until(|game| game.at(End::Server) != spawn);
    game.set_owner(Some(second));
    game.step_until(|game| (game.held(0), game.held(1)) == (Held::Unpredicted, Held::Predicted));
    // The second client orders it, predicts the walk, and agrees with the server and the first
    // client where it ends, with no more rollbacks.
    game.local
        .client_mut(1)
        .world_mut()
        .resource_mut::<PendingOrders>()
        .push(Order::one(hero, move_to(3, -1)));
    let end = Position::new(Vec3::new(Num::int(3), Num::ZERO, Num::int(-1))).unwrap();
    game.step_until(|game| {
        [End::Server, End::Client(0), End::Client(1)]
            .into_iter()
            .all(|at| game.at(at) == end)
    });
    assert_eq!((game.local.rollbacks(0), game.local.rollbacks(1)), (0, 1));

    // A unit with no owner is the server's alone: no client predicts it. Given back to the first
    // player's slot, the first client predicts it again.
    game.set_owner(None);
    game.step_until(|game| (game.held(0), game.held(1)) == (Held::Unpredicted, Held::Unpredicted));
    game.set_owner(Some(first));
    game.step_until(|game| game.held(0) == Held::Predicted);
    // No client predicts a unit whose owner has no seated link: given to the second player's
    // slot as its link is gone, it leaves the first client's prediction, and none takes it. The
    // second client runs no more frames: one whose link is cut tries it again, which this
    // scenario does not follow.
    game.local.cut_link(1);
    game.step_first_until(|game| {
        let server = game.local.server().world();
        server.get::<PlayerLink>(game.local.link(1)).is_none()
    });
    game.set_owner(Some(second));
    game.step_first_until(|game| game.held(0) == Held::Unpredicted);
    let server = game.local.server();
    let unit = server.world().entity(game.entity_in(server));
    assert!(unit.contains::<Replicate>() && !unit.contains::<PredictionTarget>());
}
