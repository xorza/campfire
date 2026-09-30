//! Whole matches of the lane mode between two scripted players, through perfect links and through
//! delayed ones, checked on the server, on both clients and in the replayed log.

use std::num::NonZeroU32;

use bevy_app::App;
use campfire_capabilities::{Dead, Team};
use campfire_math::{Num, Vec3};
use campfire_net::{LinkModel, LocalMatch, MatchSetup, OrderScript, TickHashes};
use campfire_protocol::{SeedChain, SessionLog};
use campfire_runner::{Runner, Session};
use campfire_sim::{EntityIndex, Position, SimTick, StableId};
use lightyear::prelude::{PredictionMetrics, RollbackMode};

const SEED_CHAIN: SeedChain = SeedChain::new([9; 32], NonZeroU32::MIN);
const MATCH_TICKS: u64 = 600;
/// The lane mode's respawn, 5000 ms at 30 ticks a second, from the end of the tick of death.
const RESPAWN_TICKS: u64 = 150;

/// By team, the west then the east: each hero walks 4 m toward the enemy tower, which kills it
/// there; after it respawns, it walks to a point near the middle. The first order waits for the
/// clients' lead on the server to settle: Lightyear brings it to its target by 5 % of a tick a
/// frame.
const SCRIPTS: [&str; 2] = [
    "[[order]]\ntick = 60\nmove = [4, 0]\n[[order]]\ntick = 450\nmove = [-2, 3]\n",
    "[[order]]\ntick = 60\nmove = [-4, 0]\n[[order]]\ntick = 450\nmove = [2, -3]\n",
];

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
    let mut local = LocalMatch::new(MatchSetup {
        players: 2,
        rollback: RollbackMode::Check,
        server_frames: 3,
        link,
        seed_chain: SEED_CHAIN,
    });
    local.start_match();
    // Players take slots in the order their joins arrive, so each client plays its hero's team's
    // script.
    let heroes = [local.hero(0), local.hero(1)];
    let teams = heroes.map(|id| {
        let world = local.server().world();
        let entity = world.resource::<EntityIndex>().get(id).unwrap();
        usize::from(world.get::<Team>(entity).unwrap().index())
    });
    assert_ne!(
        teams[0], teams[1],
        "the two players' heroes are on two teams"
    );
    for (client, &team) in teams.iter().enumerate() {
        local.play(client, OrderScript::parse(SCRIPTS[team]).unwrap());
    }
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

    check_log(&mut local);

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

/// Checks the server's log: every order was logged in time, and took effect in the tick of its
/// stamp; the replayed log gives the server's hash after every tick.
fn check_log(local: &mut LocalMatch) {
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
    assert_eq!(applied.len(), 4);
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
