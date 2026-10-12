//! What the MOBA 3v3's tests read of a running match beside `MatchUnits`: its units by
//! place, the players' gold and experience, and who is dead and when they come back.

use bevy_ecs::world::World;
use campfire_capabilities::{
    Dead, Experience, Life, PlayersWorld, ResourceId, Respawn, ScriptFailures, TrackId,
};
use campfire_common::{PlayerSlot, Tick};
use campfire_math::{Num, Vec3};
use campfire_runner::SessionWorld;
use campfire_runner::internals::{FixedMatch, Moba3v3};
use campfire_sim::{EntityIndex, Position, SimTick, SimWorld, StableId};

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

/// What the MOBA 3v3's tests read of a running match.
pub(crate) trait MobaWorldTest {
    /// The units that stand at `x`, `z`, by stable id.
    fn units_at(&self, x: i64, z: i64) -> Vec<StableId>;

    /// The gold player `slot` holds.
    fn gold(&self, moba: &Moba3v3, slot: u32) -> i64;

    /// The tick `id` comes back in, while it waits to.
    fn respawn_at(&self, id: StableId) -> Option<Tick>;

    /// Whether `id` is dead.
    fn dead(&self, id: StableId) -> bool;

    /// The life `id` has.
    fn life_of(&self, id: StableId) -> Num;

    /// The experience `id` has on the mode's `level` track.
    fn level_xp(&self, moba: &Moba3v3, id: StableId) -> Num;
}

impl MobaWorldTest for World {
    fn units_at(&self, x: i64, z: i64) -> Vec<StableId> {
        let pos = Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap();
        let index = self.resource::<EntityIndex>();
        index
            .iter()
            .filter(|&(_, entity)| self.get::<Position>(entity) == Some(&pos))
            .map(|(id, _)| id)
            .collect()
    }

    fn gold(&self, moba: &Moba3v3, slot: u32) -> i64 {
        let gold = ResourceId::named(&moba.packages().data().resources, "gold").unwrap();
        let held = self.player_resources(PlayerSlot::new(slot));
        held.expect("a player of the 3v3 holds its resources")
            .amount(gold)
    }

    fn respawn_at(&self, id: StableId) -> Option<Tick> {
        let entity = self.entity_of(id)?;
        self.get::<Respawn>(entity).map(|respawn| respawn.at)
    }

    fn dead(&self, id: StableId) -> bool {
        let entity = self.entity_of(id).unwrap();
        self.entity(entity).contains::<Dead>()
    }

    fn life_of(&self, id: StableId) -> Num {
        let entity = self.entity_of(id).unwrap();
        self.get::<Life>(entity).unwrap().current()
    }

    fn level_xp(&self, moba: &Moba3v3, id: StableId) -> Num {
        let tracks = &moba.packages().data().tracks;
        let at = tracks
            .keys()
            .position(|name| name.as_str() == "level")
            .unwrap();
        let entity = self.entity_of(id).unwrap();
        let experience = self.get::<Experience>(entity).unwrap();
        experience
            .get(TrackId::new(u8::try_from(at).unwrap()))
            .unwrap()
            .xp
    }
}
