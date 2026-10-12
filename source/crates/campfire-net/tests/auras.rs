//! Auras over the network (design 14, D5, D11): a client resolves an ally's aura, whose numbers
//! come from its source's stats, as the server does, so it predicts its unit inside it with no
//! correction; an enemy's aura, whose modifiers a mode hides from it, comes from the server as a
//! correction, once.

use bevy_ecs::world::EntityRef;
use campfire_capabilities::{Action, Modifiers, MoveStep};
use campfire_math::Num;
use campfire_net::OrderScript;
use campfire_net::internals::{End, InProcessMatch, MatchSetup};
use campfire_sim::{EntityIndex, StableId};
use lightyear::prelude::RollbackMode;

use crate::short_3v3::{Edit, PICKS, short_3v3};

/// Frames for the avatars to spawn and reach their clients.
const SPAWN_FRAMES: usize = 300;
/// Frames of a walk, long enough for it to end.
const WALK_FRAMES: usize = 60;

/// Gale's aura adds a hundredth of her attack damage to her allies' move speed, so its numbers
/// come from her stats; Rime's cuts the move speed of each enemy within 3 m of her by 30 %.
const AURAS: [Edit; 3] = [
    Edit {
        path: "heroes/gale/data/avatar.toml",
        from: "[modifiers.fair_wind_speed]\nstats = { move_speed = { pct = \"0.03\" } }",
        to: "[modifiers.fair_wind_speed]\nstats = { move_speed = { param = \"wind\" } }\n\n\
             [modifiers.fair_wind_speed.params]\nwind = { base = 0, attack_damage = \"0.01\" }",
    },
    Edit {
        path: "heroes/rime/data/avatar.toml",
        from: "passive = \"stillness\"",
        to: "passive = \"frost_field\"",
    },
    Edit {
        path: "heroes/rime/data/avatar.toml",
        from: "[modifiers.stillness]",
        to: "[modifiers.frost_field]\n\
             aura = { radius = \"3.0\", affects = \"enemies\", modifier = \"frost_field_slow\" }\n\n\
             [modifiers.frost_field_slow]\nstats = { move_speed = { cut = \"0.3\" } }\n\n\
             [modifiers.stillness]",
    },
];

/// The unit `id` in `local`'s `end`.
fn unit(local: &InProcessMatch, end: End, id: StableId) -> EntityRef<'_> {
    let world = local.app(end).world();
    world.entity(world.resource::<EntityIndex>().get(id).unwrap())
}

/// Whether the unit `id` carries the same modifiers, and walks the same step, on `local`'s
/// server and its client 0.
fn alike(local: &InProcessMatch, id: StableId) -> bool {
    let [server, client] = [End::Server, End::Client(0)].map(|end| unit(local, end, id));
    server.get::<Modifiers>() == client.get::<Modifiers>()
        && server.get::<MoveStep>() == client.get::<MoveStep>()
}

#[test]
fn an_allys_aura_predicts_with_no_correction_and_a_hidden_enemy_aura_corrects_once() {
    // Cinder's player alone, beside Gale at the north's spawn; Rime of the south 6 m south. The
    // mode keeps every unit's modifiers from the enemies.
    let mut setup = MatchSetup::solo(RollbackMode::Check, 1, InProcessMatch::SEED_CHAIN);
    setup.bots = &PICKS[1..];
    let hidden = "\n[visibility]\nmodifiers = [\"spectators\"]\n";
    let mut local = InProcessMatch::of_mode(setup, short_3v3(&AURAS, hidden));
    local.open_match();
    local.play(0, OrderScript::parse(PICKS[0]).unwrap());
    local.await_avatars(SPAWN_FRAMES);
    for _ in 0..WALK_FRAMES {
        local.step();
    }
    let cinder = local.avatar(0);

    // The north's heroes stand at x = -1, 0 and 1 of z = -28, Cinder, Husk and Gale; the
    // south's at z = -22, Kensho, Veil and Rime. Cinder first walks out of Gale's aura, 8 m, to
    // 10 m off her, clear of every body: a walk that sets off touching a unit its client does
    // not predict may steer round it there, and correct once, as the cast scenario shows.
    local.order(
        0,
        Action::Move {
            x: Num::int(-8),
            z: Num::int(-33),
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
        Action::Move {
            x: Num::int(-4),
            z: Num::int(-31),
        },
    );
    for _ in 0..WALK_FRAMES {
        local.step();
    }
    assert!(alike(&local, cinder));
    assert_eq!(local.rollbacks(0), settled, "Gale's aura predicted");
    let step = |local: &InProcessMatch| {
        unit(local, End::Server, cinder)
            .get::<MoveStep>()
            .unwrap()
            .get()
    };
    let in_gales = step(&local);

    // Into Rime's aura, which the client cannot see: the server's slow corrects it once, and it
    // keeps the server's instance while Cinder stands in it.
    local.order(
        0,
        Action::Move {
            x: Num::int(-1),
            z: Num::int(-24),
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
