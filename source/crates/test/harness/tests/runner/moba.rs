//! What the MOBA 3v3's tests read of a running match beside `MatchUnits`: the players' gold and
//! the heroes' experience.

use bevy_ecs::world::World;
use campfire_harness::{FixedMatch, MatchUnits, Moba3v3, ScriptedMode};
use campfire_math::Num;
use campfire_progression::{Experience, ModeTracksData};
use campfire_sim::internals::SimWorldInternals;
use campfire_sim::{SimTick, StableId};
use campfire_values::TrackId;

/// Runs `fixed` until tick `end` starts; a failed script call fails the test by its log check.
pub(crate) fn run_to(fixed: &mut FixedMatch, end: u64) {
    while now(fixed.game()) < end {
        fixed.step();
    }
}

fn now(world: &World) -> u64 {
    world.resource::<SimTick>().start().get()
}

/// The gold player `player_slot` holds in `world`, a match of the 3v3.
pub(crate) fn gold(world: &World, player_slot: u32) -> i64 {
    let units = MatchUnits::of(world);
    units.amount(player_slot, units.resource("gold").unwrap())
}

/// The experience `id` has on the `level` track of `world`, a match of `moba`.
pub(crate) fn level_xp(moba: &ScriptedMode<Moba3v3>, world: &World, id: StableId) -> Num {
    let tracks = moba
        .packages()
        .data()
        .section_or_default::<ModeTracksData>();
    let at = (tracks.0.keys())
        .position(|name| name.as_str() == "level")
        .unwrap();
    let experience = world.part::<Experience>(id);
    experience
        .get(TrackId::new(u8::try_from(at).unwrap()))
        .unwrap()
        .xp
}
