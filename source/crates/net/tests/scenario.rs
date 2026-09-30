//! Whole matches of the lane mode between two scripted players, through perfect links and through
//! delayed ones, checked on the server, on both clients and in the replayed log.

use std::num::NonZeroU32;

use bevy_app::App;
use campfire_capabilities::{AbilitySlots, Dead, Health, ResourcePool};
use campfire_math::{Num, Vec3};
use campfire_net::{LinkModel, LocalMatch, MatchSetup, TickHashes};
use campfire_protocol::{SeedChain, SessionLog};
use campfire_runner::{Runner, Session};
use campfire_sim::{EntityIndex, Position, SimTick, StableId, Tick};
use lightyear::prelude::PredictionMetrics;

const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
const MATCH_TICKS: u64 = 600;
/// The lane mode's respawn, 5000 ms at 30 ticks a second, from the end of the tick of death.
const RESPAWN_TICKS: u64 = 150;

fn at(x: i64, z: i64) -> Position {
    let num = |value| Num::from_int(value).unwrap();
    Position::new(Vec3::new(num(x), Num::ZERO, num(z))).unwrap()
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

/// The sim tick an app runs next.
fn next_tick(app: &App) -> u64 {
    app.world().resource::<SimTick>().start().get()
}

/// When a hero died and came back in one app, by that app's ticks.
#[derive(Debug, Default, Clone, Copy)]
struct Life {
    died: Option<u64>,
    back: Option<u64>,
}

impl Life {
    /// Notes the hero's state after `tick`; `true` when it just died.
    fn note(&mut self, dead: bool, tick: u64) -> bool {
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
    let mut local = LocalMatch::new(MatchSetup::duo(link, SEED_CHAIN));
    local.start_match();
    let heroes = [local.hero(0), local.hero(1)];
    let teams = local.play_by_team(LocalMatch::SCENARIO_SCRIPTS);
    let mut on_server = [Life::default(); 2];
    let mut on_client = [Life::default(); 2];
    // Each client's lead on the server, in ticks, when it learned its hero died; below 0 when it
    // runs behind.
    let mut lead = [0_i64; 2];
    while next_tick(local.server()) < MATCH_TICKS {
        local.step();
        let server_tick = next_tick(local.server());
        for (index, &id) in heroes.iter().enumerate() {
            on_server[index].note(hero(local.server(), id).dead, server_tick - 1);
            let client = local.client(index);
            let client_tick = next_tick(client);
            // A client that has not run a tick of the match yet has nothing to note.
            let Some(ran) = client_tick.checked_sub(1) else {
                continue;
            };
            if on_client[index].note(hero(client, id).dead, ran) {
                lead[index] = signed(client_tick) - signed(server_tick);
            }
        }
    }
    // Let the clients receive the last ticks.
    for _ in 0..20 {
        local.step();
    }

    check_log(&mut local, 4);

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
        let end = [at(-2, 3), at(2, -3)][team];
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

    [0, 1].map(|client| {
        let world = local.client(client).world();
        world.resource::<PredictionMetrics>().rollbacks
    })
}

/// Checks the server's log: all `inputs` orders were logged in time, and took effect in the tick
/// of their stamps; the replayed log gives the server's hash after every tick.
fn check_log(local: &mut LocalMatch, inputs: usize) {
    let server = local.server_mut().world_mut();
    server.resource_mut::<Session>().reveal_seed();
    let live = server.resource::<TickHashes>().get().to_vec();
    let mut file = Vec::new();
    server.resource::<Session>().log().encode(&mut file);
    let decoded = SessionLog::decode(&file).unwrap();
    let seed = decoded.revealed_seed().unwrap();
    let ticks = decoded.next_tick();
    let mut rewound = decoded.rewound();
    let mut applied = Vec::new();
    for tick in 0..ticks {
        applied.extend(rewound.seal_tick().map(|input| (input.stamp, tick)));
    }
    assert_eq!(applied.len(), inputs);
    assert!(
        applied.iter().all(|&(stamp, tick)| stamp == tick),
        "{applied:?}"
    );
    let decoded = SessionLog::decode(&file).unwrap();
    let mut replay = Runner::new(decoded.rewound(), seed, local.packages()).unwrap();
    for (tick, live) in live.iter().enumerate() {
        replay.run_tick();
        assert_eq!(replay.state_hash(), *live, "tick {tick}");
    }
}

/// By team, the west, whose hero is the walker, then the east: both walk 7.5 m off the lane, out of
/// reach of creeps and towers and within the vision grid, to stand 1 m apart. Then the walker casts its first ability, 100
/// true damage within 2 m for 40 of its 100 mana, every 90 ticks: in tick 100 it hits; in 120 it
/// is on cooldown; in 190 its cooldown has ended, and it hits; in 280 the 20 mana left do not pay.
const CAST_SCRIPTS: [&str; 2] = [
    "[[order]]\ntick = 60\nmove = [0, \"7.5\"]\n[[order]]\ntick = 100\ncast = 0\n[[order]]\ntick = 120\ncast = 0\n[[order]]\ntick = 190\ncast = 0\n[[order]]\ntick = 280\ncast = 0\n",
    "[[order]]\ntick = 60\nmove = [1, \"7.5\"]\n",
];
const CAST_TICKS: u64 = 300;

/// A unit's health, its resource, and the first tick its first ability may be cast again, as an
/// app holds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Caster {
    health: Num,
    resource: Option<Num>,
    ready_at: Option<Tick>,
}

fn caster(app: &App, id: StableId) -> Caster {
    let world = app.world();
    let unit = world.entity(world.resource::<EntityIndex>().get(id).unwrap());
    Caster {
        health: unit.get::<Health>().unwrap().current(),
        resource: unit.get::<ResourcePool>().map(|pool| pool.current()),
        ready_at: unit
            .get::<AbilitySlots>()
            .and_then(|slots| slots.slot(0))
            .map(|slot| slot.ready_at),
    }
}

/// Plays the cast scenario through `link` and checks it; gives each client's rollbacks.
fn cast(link: LinkModel) -> [u32; 2] {
    let mut local = LocalMatch::new(MatchSetup::duo(link, SEED_CHAIN));
    local.start_match();
    // Player 0 plays the walker, of the west.
    let heroes = [local.hero(0), local.hero(1)];
    let teams = local.play_by_team(CAST_SCRIPTS);
    let walker = heroes[teams.iter().position(|&team| team == 0).unwrap()];
    let runner = heroes[teams.iter().position(|&team| team == 1).unwrap()];
    // The ticks after which the runner's health changed on the server.
    let mut hit = Vec::new();
    let mut health = caster(local.server(), runner).health;
    while next_tick(local.server()) < CAST_TICKS {
        local.step();
        let now = caster(local.server(), runner).health;
        if now != health {
            hit.push(next_tick(local.server()) - 1);
            health = now;
        }
    }
    for _ in 0..20 {
        local.step();
    }
    check_log(&mut local, 6);

    // Each cast that paid took effect in its stamp tick: 600 − 100 − 100 health; 100 − 40 − 40
    // mana, and the ability ready again 90 ticks after the second, on the server and on both
    // clients.
    assert_eq!(hit, [100, 190]);
    let num = |value| Num::from_int(value).unwrap();
    let expected = [
        (
            runner,
            Caster {
                health: num(400),
                resource: Some(num(100)),
                ready_at: Some(Tick::ZERO),
            },
        ),
        (
            walker,
            Caster {
                health: num(600),
                resource: Some(num(20)),
                ready_at: Some(Tick::new(280)),
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
    [0, 1].map(|client| {
        let world = local.client(client).world();
        world.resource::<PredictionMetrics>().rollbacks
    })
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
    let rollbacks = play(LinkModel {
        delay: 3,
        jitter: 2,
        loss_per_mille: 0,
        seed: 7,
    });
    assert_eq!(rollbacks, [1, 1]);
}

#[test]
fn a_cast_through_delayed_links() {
    let rollbacks = cast(LinkModel {
        delay: 3,
        jitter: 2,
        loss_per_mille: 0,
        seed: 7,
    });
    assert_eq!(rollbacks, [0, 0]);
}
