//! Whole matches of the lane mode between two scripted players, through perfect links and through
//! delayed ones, checked on the server, on both clients and in the replayed log.

use bevy_app::App;
use campfire_capabilities::{ActionSlots, Body, Dead, PoolId, Pools, SeenBy, Team};
use campfire_common::Tick;
use campfire_math::{Num, Vec3};
use campfire_net::internals::{End, LinkModel, LocalMatch, MatchSetup};
use campfire_net::{OrderScript, TickHashes};
use campfire_protocol::SessionLog;
use campfire_runner::internals::HashTrail;
use campfire_runner::{Runner, Session};
use campfire_sim::{EntityIndex, Position, StableId};

const MATCH_TICKS: u64 = 600;
/// The lane mode's respawn, 5000 ms at 30 ticks a second, from the end of the tick of death.
const RESPAWN_TICKS: u64 = 150;

const fn at(x: i64, z: i64) -> Position {
    Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap()
}

/// Where a hero stands in an app, and whether it is dead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hero {
    position: Position,
    dead: bool,
}

fn hero(app: &App, id: StableId) -> Hero {
    let world = app.world();
    let unit = world.entity(world.resource::<EntityIndex>().get(id).unwrap());
    Hero {
        position: *unit.get::<Position>().unwrap(),
        dead: unit.contains::<Dead>(),
    }
}

fn signed(tick: u64) -> i64 {
    i64::try_from(tick).unwrap()
}

/// When a hero died and came back in one app, by that app's ticks.
#[derive(Debug, Default, Clone, Copy)]
struct Life {
    died: Option<u64>,
    back: Option<u64>,
}

impl Life {
    /// Notes the hero's state after `tick`; `true` when it just died.
    const fn note(&mut self, dead: bool, tick: u64) -> bool {
        if dead && self.died.is_none() {
            self.died = Some(tick);
            return true;
        }
        if !dead && self.died.is_some() && self.back.is_none() {
            self.back = Some(tick);
        }
        false
    }
}

/// Plays the match through `link` and checks it; gives each client's rollbacks.
fn play(link: LinkModel) -> [u32; 2] {
    let mut local = LocalMatch::new(MatchSetup::duo(link, LocalMatch::SEED_CHAIN));
    local.start_match();
    let heroes = [local.avatar(0), local.avatar(1)];
    let teams = local.play_by_team(LocalMatch::SCENARIO_SCRIPTS);
    let mut on_server = [Life::default(); 2];
    let mut on_client = [Life::default(); 2];
    // Each client's lead on the server, in ticks, when it learned its hero died; below 0 when it
    // runs behind.
    let mut lead = [0_i64; 2];
    while local.next_tick(End::Server) < MATCH_TICKS {
        local.step();
        let server_tick = local.next_tick(End::Server);
        for (index, &id) in heroes.iter().enumerate() {
            on_server[index].note(hero(local.server(), id).dead, server_tick - 1);
            let client = local.client(index);
            let client_tick = local.next_tick(End::Client(index));
            // A client that has not run a tick of the match yet has nothing to note.
            let Some(ran) = client_tick.checked_sub(1) else {
                continue;
            };
            if on_client[index].note(hero(client, id).dead, ran) {
                lead[index] = signed(client_tick) - signed(server_tick);
            }
        }
    }

    check_log(&mut local, LocalMatch::SCENARIO_SCRIPTS);

    let worst = u64::from(link.delay + link.jitter);
    for index in 0..2 {
        let died = on_server[index].died.expect("the tower kills the hero");
        // Back at the start of the tick 1 + 150 after the tick of death, on the server and on its
        // client, which learned the respawn ahead.
        assert_eq!(
            on_server[index].back,
            Some(died + 1 + RESPAWN_TICKS),
            "hero {index}"
        );
        assert_eq!(
            on_client[index].back, on_server[index].back,
            "client {index}"
        );
        // A client learns of its own death at its own tick, which leads the server's, once the
        // state crossed the downlink: its lead, then the downlink's delay up to its worst case,
        // and a frame for the state to apply, after the death.
        let learned = signed(on_client[index].died.unwrap()) - lead[index];
        let died = signed(died);
        assert!(
            (died..=died + signed(worst) + 1).contains(&learned),
            "client {index}: {learned} after {died}, lead {}",
            lead[index]
        );
    }
    // Each ends where its last order sent it, alive, on the server and on its client.
    for (index, (&id, &team)) in heroes.iter().zip(&teams).enumerate() {
        let end = [at(-3, -2), at(3, 4)][team];
        let end = Hero {
            position: end,
            dead: false,
        };
        assert_eq!(hero(local.server(), id), end, "hero {index} on the server");
        assert_eq!(
            hero(local.client(index), id),
            end,
            "hero {index} on its client"
        );
    }

    [0, 1].map(|client| local.rollbacks(client))
}

/// Checks the server's log: every order of the two `scripts` was logged in time, and took effect
/// in the tick of its stamp; the replayed log gives the server's hash after every tick.
fn check_log(local: &mut LocalMatch, scripts: [&str; 2]) {
    let inputs: usize = scripts
        .map(|script| OrderScript::parse(script).unwrap().orders().len())
        .iter()
        .sum();
    let server = local.server_mut().world_mut();
    server.resource_mut::<Session>().reveal_seed();
    let live = HashTrail::of_totals(server.resource::<TickHashes>().get().to_vec());
    let mut file = Vec::new();
    server.resource::<Session>().log().encode(&mut file);
    let decoded = SessionLog::decode(&file).unwrap();
    let seed = decoded.revealed_seed().unwrap();
    let ticks = decoded.next_tick();
    let mut rewound = decoded.rewound();
    let mut applied = Vec::new();
    for tick in 0..ticks.get() {
        applied.extend(rewound.seal_tick().map(|input| (input.stamp, tick)));
    }
    assert_eq!(applied.len(), inputs, "{applied:?}");
    assert!(
        applied
            .iter()
            .all(|&(stamp, tick)| stamp == Tick::new(tick)),
        "{applied:?}"
    );
    let decoded = SessionLog::decode(&file).unwrap();
    let mut replay = Runner::new(decoded.rewound(), seed, local.packages()).unwrap();
    let mut replayed = HashTrail::default();
    for _ in live.totals() {
        replay.run_tick();
        replayed.record(replay.world());
    }
    live.assert_same(&replayed);
    assert_eq!(replay.log().next_tick(), ticks);
}

/// The orders by team, the west, whose hero is the walker, then the east, whose is the runner:
/// both order a point past the map's edge at z = 12, and stop on the edge, 8 m off the lane, out
/// of reach of creeps and towers, 1.5 m apart, so their bodies of 0.5 m never touch; there each
/// sees the other.
///
/// Each first learns its first ability, with the point it spawns with, in tick 60.
///
/// The walker casts its first ability, 100 true damage within 2 m for 40 of its 100 mana, every
/// 90 ticks: in tick 100 it hits; in 120 it is on cooldown; in 190 its cooldown has ended, and it
/// hits; in 280 the 20 mana left do not pay. In tick 300 it attacks the runner: 60 damage 8 ticks
/// into each attack of 20, in 308, 328 and 348; in 350 it walks to where it stands, which ends
/// the attack.
///
/// The runner casts its first ability at the walker in tick 140: 80 true damage for 30 of its 100
/// mana, every 60 ticks.
fn cast_scripts(walker: StableId, runner: StableId) -> [String; 2] {
    let walker_orders = format!(
        "[[order]]\ntick = 60\nlearn = 0\n[[order]]\ntick = 60\nmove = [0, 12]\n\
         [[order]]\ntick = 100\ncast = 0\n[[order]]\ntick = 120\ncast = 0\n\
         [[order]]\ntick = 190\ncast = 0\n[[order]]\ntick = 280\ncast = 0\n\
         [[order]]\ntick = 300\nattack = {}\n[[order]]\ntick = 350\nmove = [0, 12]\n",
        runner.get()
    );
    let runner_orders = format!(
        "[[order]]\ntick = 60\nlearn = 0\n[[order]]\ntick = 60\nmove = [\"1.5\", 12]\n\
         [[order]]\ntick = 140\ncast = 0\ntarget = {}\n",
        walker.get()
    );
    [walker_orders, runner_orders]
}
const CAST_TICKS: u64 = 360;

/// Where a unit stands, its health and mana, the lane mode's pools in the order of their names,
/// and the first tick its first ability may be cast again, as an app holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Caster {
    pos: Position,
    health: Num,
    mana: Option<Num>,
    ready_at: Option<Tick>,
}

fn caster(app: &App, id: StableId) -> Caster {
    let world = app.world();
    let unit = world.entity(world.resource::<EntityIndex>().get(id).unwrap());
    let pools = unit.get::<Pools>().unwrap();
    Caster {
        pos: *unit.get::<Position>().unwrap(),
        health: pools.current(PoolId::FIRST).unwrap(),
        mana: pools.current(PoolId::new(1).unwrap()),
        ready_at: unit
            .get::<ActionSlots>()
            .and_then(|slots| slots.slot(0))
            .map(|slot| slot.ready_at),
    }
}

/// Plays the cast and attack scenario through `link` and checks it; gives the rollbacks of the
/// walker's client, then of the runner's.
fn cast(link: LinkModel) -> [u32; 2] {
    let mut local = LocalMatch::new(MatchSetup::duo(link, LocalMatch::SEED_CHAIN));
    local.start_match();
    // Player 0 plays the walker, of the west.
    let clients = [0, 1].map(|team| {
        (0..2)
            .find(|&client| local.team(client).index() == team)
            .unwrap()
    });
    let [walker, runner] = clients.map(|client| local.avatar(client));
    let [west, east] = cast_scripts(walker, runner);
    local.play_by_team([&west, &east]);
    // The ticks after which each hero's health changed on the server, the walker's then the
    // runner's.
    let mut hits = [Vec::new(), Vec::new()];
    let mut health = [walker, runner].map(|id| caster(local.server(), id).health);
    while local.next_tick(End::Server) < CAST_TICKS {
        local.step();
        for (index, id) in [walker, runner].into_iter().enumerate() {
            let now = caster(local.server(), id).health;
            if now != health[index] {
                hits[index].push(local.next_tick(End::Server) - 1);
                health[index] = now;
            }
        }
    }
    check_log(&mut local, [&west, &east]);

    // Each order took effect in its stamp tick: each hero stands on the edge, z = 8; the walker
    // lost 80 health to the runner's cast;
    // the runner 100 to each of the walker's two casts that paid, and 60 to each of its three
    // strikes. The walker spent 40 mana twice, the runner 30 once, and each ability is ready
    // again after its last cast's cooldown: on the server and on both clients.
    assert_eq!(hits, [vec![140], vec![100, 190, 308, 328, 348]]);
    let edge = |x| Position::new(Vec3::new(x, Num::ZERO, Num::int(8))).unwrap();
    let expected = [
        (
            walker,
            Caster {
                pos: edge(Num::ZERO),
                health: Num::int(600 - 80),
                mana: Some(Num::int(100 - 40 - 40)),
                ready_at: Some(Tick::new(190 + 90)),
            },
        ),
        (
            runner,
            Caster {
                pos: edge(Num::int(1) + Num::HALF),
                health: Num::int(600 - 100 - 100 - 3 * 60),
                mana: Some(Num::int(100 - 30)),
                ready_at: Some(Tick::new(140 + 60)),
            },
        ),
    ];
    for (id, expected) in expected {
        assert_eq!(caster(local.server(), id), expected, "{id:?} on the server");
        for client in 0..2 {
            assert_eq!(
                caster(local.client(client), id),
                expected,
                "{id:?} on client {client}"
            );
        }
    }
    // On the edge, in the grid's last row, each is seen by both teams.
    for id in [walker, runner] {
        let world = local.server().world();
        let unit = world.entity(world.resource::<EntityIndex>().get(id).unwrap());
        let seen = unit.get::<SeenBy>().unwrap().get();
        let teams = [0, 1].map(|team| seen.contains(Team::new(team)));
        assert_eq!(teams, [true, true], "{id:?}");
    }
    clients.map(|client| local.rollbacks(client))
}

/// The ticks of the route scenario: the west hero walks some 12 m at 0.117 m a tick from tick 60.
const ROUTE_TICKS: u64 = 240;

/// Plays the route scenario through `link` and checks it; gives each client's rollbacks. In tick
/// 60 the west hero, from (0, −2), is ordered to (−12, −4), behind its own tower at (−8, −3),
/// whose body the straight line passes a third of a meter from its center; the east hero walks
/// off to (12, 7), out of every unit's reach.
fn route(link: LinkModel) -> [u32; 2] {
    let mut local = LocalMatch::new(MatchSetup::duo(link, LocalMatch::SEED_CHAIN));
    local.start_match();
    let scripts = [
        "[[order]]\ntick = 60\nmove = [-12, -4]\n",
        "[[order]]\ntick = 60\nmove = [12, 7]\n",
    ];
    local.play_by_team(scripts);
    let client = local.client_of(Team::new(0));
    let walker = local.avatar(client);
    let world = local.server().world();
    let tower = world.entity(
        world
            .resource::<EntityIndex>()
            .get(local.tower(Team::new(0)))
            .unwrap(),
    );
    let tower_at = tower.get::<Position>().unwrap().get();
    let walker_body = world.entity(world.resource::<EntityIndex>().get(walker).unwrap());
    let reach = tower.get::<Body>().unwrap().radius() + walker_body.get::<Body>().unwrap().radius();
    let reach = u128::from(reach.to_bits().unsigned_abs());
    let mut closest = u128::MAX;
    while local.next_tick(End::Server) < ROUTE_TICKS {
        local.step();
        let offset = hero(local.server(), walker).position.get() - tower_at;
        closest = closest.min(offset.length_squared_bits());
    }
    check_log(&mut local, scripts);

    // It went round the tower, never touching it, and stands on its goal, on the server and on
    // its client.
    assert!(
        closest >= reach * reach,
        "{closest} against {}",
        reach * reach
    );
    let end = Hero {
        position: at(-12, -4),
        dead: false,
    };
    assert_eq!(hero(local.server(), walker), end);
    assert_eq!(hero(local.client(client), walker), end);
    [0, 1].map(|client| local.rollbacks(client))
}

/// The ticks of the steering scenario: the creeps, 7.25 m off the middle, have yet to come within
/// their 6 m aggro range of a hero, 6.85 m between centres.
const ROUND_TICKS: u64 = 70;

/// Plays the steering scenario through `link` and checks it; gives the rollbacks of the walker's
/// client, then of the runner's. In tick 1 the west hero, from (0, −2), is ordered to (0, 5),
/// through the east hero, which stands on its spawn at (0, 2).
fn round(link: LinkModel) -> [u32; 2] {
    let mut local = LocalMatch::new(MatchSetup::duo(link, LocalMatch::SEED_CHAIN));
    local.start_match();
    let scripts = ["[[order]]\ntick = 1\nmove = [0, 5]\n", ""];
    local.play_by_team(scripts);
    let clients = [0, 1].map(|team| local.client_of(Team::new(team)));
    let [walker, stander] = clients.map(|client| local.avatar(client));
    let world = local.server().world();
    let radius = |id| {
        let unit = world.entity(world.resource::<EntityIndex>().get(id).unwrap());
        unit.get::<Body>().unwrap().radius()
    };
    let reach = radius(walker) + radius(stander);
    let reach = u128::from(reach.to_bits().unsigned_abs());
    let mut closest = u128::MAX;
    while local.next_tick(End::Server) < ROUND_TICKS {
        local.step();
        let [walking, standing] = [walker, stander].map(|id| hero(local.server(), id).position);
        closest = closest.min(walking.ground_offset(standing).length_squared_bits());
    }
    check_log(&mut local, scripts);

    // It went round the hero that stands, never touching it, and stands on its goal; the other
    // never moved: on the server and on both clients.
    assert!(
        closest >= reach * reach,
        "{closest} against {}",
        reach * reach
    );
    for (id, end) in [(walker, at(0, 5)), (stander, at(0, 2))] {
        let end = Hero {
            position: end,
            dead: false,
        };
        assert_eq!(hero(local.server(), id), end, "{id:?} on the server");
        for client in 0..2 {
            assert_eq!(
                hero(local.client(client), id),
                end,
                "{id:?} on client {client}"
            );
        }
    }
    clients.map(|client| local.rollbacks(client))
}

#[test]
fn a_hero_round_a_hero_that_stands_through_delayed_links() {
    // The walker's client sees the other hero stand where the server has it, so it plans the same
    // way round, and neither client corrects.
    let rollbacks = round(LinkModel::DELAYED);
    assert_eq!(rollbacks, [0, 0]);
}

#[test]
fn a_route_round_a_tower_through_delayed_links() {
    // The client plans its hero's route as the server does, so it corrects nothing.
    let rollbacks = route(LinkModel::DELAYED);
    assert_eq!(rollbacks, [0, 0]);
}

#[test]
fn a_server_stall_runs_at_most_its_bound_and_makes_no_input_late() {
    // The lane mode at 30 ticks a second, with a max input delay of 10 ticks: a frame advances
    // the server 9 ticks at most. A stall of 8 ticks, as the first orders go out in tick 60,
    // runs all 8 in one frame, and the orders it held are read after them, 8 ticks late at most:
    // none is late, and the match plays out as it does with no stall.
    let mut local = LocalMatch::new(MatchSetup::duo(LinkModel::PERFECT, LocalMatch::SEED_CHAIN));
    local.start_match();
    let heroes = [local.avatar(0), local.avatar(1)];
    let teams = local.play_by_team(LocalMatch::SCENARIO_SCRIPTS);
    while local.next_tick(End::Server) < 56 {
        local.step();
    }
    let ran = |local: &mut LocalMatch, ticks: u32| {
        let before = local.next_tick(End::Server);
        local.stall_server(ticks);
        local.next_tick(End::Server) - before
    };
    assert_eq!(ran(&mut local, 8), 8);
    while local.next_tick(End::Server) < MATCH_TICKS {
        local.step();
    }
    // Each hero ends where its last order sent it, on the server and on its client.
    for (index, (&id, &team)) in heroes.iter().zip(&teams).enumerate() {
        let end = Hero {
            position: [at(-3, -2), at(3, 4)][team],
            dead: false,
        };
        assert_eq!(hero(local.server(), id), end, "hero {index} on the server");
        assert_eq!(
            hero(local.client(index), id),
            end,
            "hero {index} on its client"
        );
    }
    // A stall of 2 s, 60 ticks, runs 9, and drops the rest.
    assert_eq!(ran(&mut local, 60), 9);
}

#[test]
fn a_1v1_through_perfect_links() {
    // Each client corrects once, when it learns its own hero died.
    assert_eq!(play(LinkModel::PERFECT), [1, 1]);
}

#[test]
fn a_1v1_through_delayed_links() {
    // 3 steps each way and up to 2 more: the orders still land in time, and each client still
    // corrects only for its hero's death.
    let rollbacks = play(LinkModel::DELAYED);
    assert_eq!(rollbacks, [1, 1]);
}

#[test]
fn a_cast_through_delayed_links() {
    let rollbacks = cast(LinkModel::DELAYED);
    // The casts need no correction. In tick 60 the walker sets off north through the runner's
    // spawn as the runner leaves it; its client, which has yet to learn the runner's order, sees
    // the runner stand in its way and goes round it, and corrects once to the server's straight
    // walk.
    assert_eq!(rollbacks, [1, 0]);
}
