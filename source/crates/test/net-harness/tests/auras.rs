//! Auras over the network (10-replication-audiences.md, A mode's hidden data is not predicted;
//! 10-replication-audiences.md, A client derives what it can for every unit it holds): a client
//! resolves an ally's aura, whose numbers come from its source's stats, as the server does, so it
//! predicts its unit inside it with no correction; an enemy's aura, whose modifiers a mode hides
//! from it, comes from the server as a correction, once.

use campfire_bots::OrderScript;
use campfire_math::Num;
use campfire_net_harness::{End, InProcessMatch, InProcessSetup};
use campfire_net_prediction::RollbackMode;
use campfire_orders::OrdersDirective;
use campfire_sim::StableId;
use campfire_stats::Modifiers;
use campfire_test_packages::Edit;
use campfire_units::{GroundPoint, MoveStep};

use crate::short_3v3::{PICKS, short_3v3};

/// Frames for the avatars to spawn and reach their clients.
const SPAWN_FRAMES: usize = 300;
/// Frames of a walk, long enough for it to end.
const WALK_FRAMES: usize = 60;

/// Gale's aura adds a hundredth of her attack damage to her allies' move speed, so its numbers
/// come from her stats; Rime's cuts the move speed of each enemy within 3 m of her by 30 %.
const AURAS: [(&str, Edit<'_>); 3] = [
    (
        "moba/heroes/gale/data/avatar.toml",
        Edit::Replace(
            "[modifiers.fair_wind_speed]\nstats = { move_speed = { pct = \"0.03\" } }",
            "[modifiers.fair_wind_speed]\nstats = { move_speed = { param = \"wind\" } }\n\n\
             [modifiers.fair_wind_speed.params]\nwind = { base = 0, attack_damage = \"0.01\" }",
        ),
    ),
    (
        "moba/heroes/rime/data/avatar.toml",
        Edit::Replace("passive = \"stillness\"", "passive = \"frost_field\""),
    ),
    (
        "moba/heroes/rime/data/avatar.toml",
        Edit::Replace(
            "[modifiers.stillness]",
            "[modifiers.frost_field]\n\
             aura = { radius = \"3.0\", affects = \"enemies\", modifier = \"frost_field_slow\" }\n\n\
             [modifiers.frost_field_slow]\nstats = { move_speed = { cut = \"0.3\" } }\n\n\
             [modifiers.stillness]",
        ),
    ),
];

/// Whether the unit `id` carries the same modifiers, and walks the same step, on `local`'s
/// server and its client 0.
fn alike(local: &InProcessMatch, id: StableId) -> bool {
    let [server, client] = [End::Server, End::Client(0)].map(|end| local.unit(end, id));
    server.get::<Modifiers>() == client.get::<Modifiers>()
        && server.get::<MoveStep>() == client.get::<MoveStep>()
}

#[test]
fn an_allys_aura_predicts_with_no_correction_and_a_hidden_enemy_aura_corrects_once() {
    // Cinder's player alone, beside Gale at the north's spawn; Rime of the south 6 m south. The
    // mode keeps every unit's modifiers from the enemies.
    let setup = InProcessSetup::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN)
        .into_builder()
        .bots(&PICKS[1..])
        .build();
    let hidden = "\n[visibility]\nmodifiers = [\"spectators\"]\n";
    let mut local = InProcessMatch::of_mode(setup, short_3v3(&AURAS, hidden));
    local.open_match();
    local.play(0, &OrderScript::parse(PICKS[0]).unwrap());
    local.await_avatars(SPAWN_FRAMES);
    for _ in 0..WALK_FRAMES {
        local.step();
    }
    let cinder = local.avatar(0);

    // The north's heroes stand at the room nearest its spawn, (0, -28), a meter clear of each
    // other: Cinder there, Gale at x = 2, Husk at z = -30; the south's round (0, -22), Kensho,
    // Rime at x = 2, and Veil at z = -24. Cinder first walks out of Gale's aura, 8 m, to 10 m off
    // her, clear of every body: a walk that sets off touching a unit its client does
    // not predict may steer round it there, and correct once, as the cast scenario shows.
    local.order(
        0,
        OrdersDirective::Move {
            to: GroundPoint {
                x: Num::int(-8),
                z: Num::int(-28),
            },
        },
    );
    for _ in 0..WALK_FRAMES {
        local.step();
    }
    let settled = local.rollbacks(0);
    // Into Gale's aura, 6 m off her, whose step the client resolves from her stats ahead of the
    // server: it walks Cinder as the server does, at the server's step, and corrects nothing.
    local.order(
        0,
        OrdersDirective::Move {
            to: GroundPoint {
                x: Num::int(-4),
                z: Num::int(-28),
            },
        },
    );
    for _ in 0..WALK_FRAMES {
        local.step();
    }
    assert!(alike(&local, cinder));
    assert_eq!(local.rollbacks(0), settled, "Gale's aura predicted");
    let step = |local: &InProcessMatch| {
        local
            .unit(End::Server, cinder)
            .get::<MoveStep>()
            .unwrap()
            .get()
    };
    let in_gales = step(&local);

    // Into Rime's aura, which the client cannot see: the server's slow corrects it once, and it
    // keeps the server's instance while Cinder stands in it.
    local.order(
        0,
        OrdersDirective::Move {
            to: GroundPoint {
                x: Num::int(3),
                z: Num::int(-24),
            },
        },
    );
    for _ in 0..WALK_FRAMES {
        local.step();
    }
    assert!(alike(&local, cinder));
    assert!(step(&local) < in_gales, "Rime's aura slows Cinder");
    assert_eq!(
        local.rollbacks(0),
        settled + 1,
        "Rime's aura corrected once"
    );
}
