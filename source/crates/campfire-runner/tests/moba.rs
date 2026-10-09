//! What the MOBA 3v3's tests read of a running match beside `MatchUnits`: its units by
//! place, the players' gold and experience, and who is dead and when they come back.

use bevy_ecs::world::World;
use campfire_capabilities::{
    Dead, Experience, PlayerResources, PoolId, Pools, ResourceId, Respawn, ScriptFailures, TrackId,
};
use campfire_common::{PlayerSlot, Tick};
use campfire_math::{Num, Vec3};
use campfire_runner::internals::{FixedMatch, Moba3v3};
use campfire_sim::{EntityIndex, Position, SimTick, StableId};

/// Runs `fixed` until tick `end` starts, checking that no script call fails.
pub(crate) fn run_to(fixed: &mut FixedMatch, end: u64) {
    while now(fixed.runner().world()) < end {
        fixed.runner_mut().run_tick();
        let failures = fixed.runner().world().non_send::<ScriptFailures>();
        assert!(failures.get().is_empty(), "{:?}", failures.get());
    }
}

fn now(world: &World) -> u64 {
    world.resource::<SimTick>().start().get()
}

/// The units that stand at `x`, `z`, by stable id.
pub(crate) fn units_at(world: &World, x: i64, z: i64) -> Vec<StableId> {
    let at = Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap();
    let index = world.resource::<EntityIndex>();
    index
        .iter()
        .filter(|&(_, entity)| world.get::<Position>(entity) == Some(&at))
        .map(|(id, _)| id)
        .collect()
}

pub(crate) fn gold(world: &World, moba: &Moba3v3, slot: u32) -> i64 {
    let gold = ResourceId::named(&moba.packages().data().resources, "gold").unwrap();
    world
        .resource::<PlayerResources>()
        .amount(PlayerSlot::new(slot), gold)
}

pub(crate) fn respawn_at(world: &World, unit: StableId) -> Option<Tick> {
    let entity = world.resource::<EntityIndex>().get(unit)?;
    world.get::<Respawn>(entity).map(|respawn| respawn.at)
}

pub(crate) fn dead(world: &World, unit: StableId) -> bool {
    let entity = world.resource::<EntityIndex>().get(unit).unwrap();
    world.entity(entity).contains::<Dead>()
}

/// The life `unit` has, of the mode's pool `health`.
pub(crate) fn life(world: &World, moba: &Moba3v3, unit: StableId) -> Num {
    let health = PoolId::named(&moba.packages().data().pools, "health").unwrap();
    let entity = world.resource::<EntityIndex>().get(unit).unwrap();
    world.get::<Pools>(entity).unwrap().current(health).unwrap()
}

/// The experience `unit` has on the mode's `level` track.
pub(crate) fn level_xp(world: &World, moba: &Moba3v3, unit: StableId) -> Num {
    let tracks = &moba.packages().data().tracks;
    let at = tracks
        .keys()
        .position(|name| name.as_str() == "level")
        .unwrap();
    let entity = world.resource::<EntityIndex>().get(unit).unwrap();
    let experience = world.get::<Experience>(entity).unwrap();
    experience
        .get(TrackId::new(u8::try_from(at).unwrap()))
        .unwrap()
        .xp
}
