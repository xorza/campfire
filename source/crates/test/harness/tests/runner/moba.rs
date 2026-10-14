//! What the MOBA 3v3's tests read of a running match beside `MatchUnits`: the players' gold and
//! the heroes' experience.

use bevy_ecs::world::World;
use campfire_common::PlayerSlot;
use campfire_harness::{FixedMatch, Moba3v3, ScriptedMode};
use campfire_math::Num;
use campfire_players::PlayersWorld;
use campfire_progression::Experience;
use campfire_runner::SessionWorld;
use campfire_script::ScriptFailures;
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{SimTick, StableId};
use campfire_values::{DeclaredName, Names, ResourceId, TrackId};

/// Runs `fixed` until tick `end` starts, checking that no script call fails.
pub(crate) fn run_to(fixed: &mut FixedMatch, end: u64) {
    while now(fixed.game()) < end {
        fixed.game_mut().run_tick_alone();
        let failures = fixed.game().non_send::<ScriptFailures>();
        assert!(failures.get().is_empty(), "{:?}", failures.get());
    }
}

fn now(world: &World) -> u64 {
    world.resource::<SimTick>().start().get()
}

/// The gold player `slot` holds in `world`, a match of `moba`.
pub(crate) fn gold(moba: &ScriptedMode<Moba3v3>, world: &World, slot: u32) -> i64 {
    let resources = moba.packages().data().resources.iter();
    let resources: Names<ResourceId> = resources.map(DeclaredName::as_str).collect();
    let gold = resources.named("gold").unwrap();
    let held = world.player_resources(PlayerSlot::new(slot));
    held.expect("a player of the 3v3 holds its resources")
        .amount(gold)
}

/// The experience `id` has on the `level` track of `world`, a match of `moba`.
pub(crate) fn level_xp(moba: &ScriptedMode<Moba3v3>, world: &World, id: StableId) -> Num {
    let tracks = &moba.packages().data().tracks;
    let at = tracks
        .keys()
        .position(|name| name.as_str() == "level")
        .unwrap();
    let experience = world.part::<Experience>(id);
    experience
        .get(TrackId::new(u8::try_from(at).unwrap()))
        .unwrap()
        .xp
}
