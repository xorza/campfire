use bevy_ecs::world::World;
use campfire_core::{TagProperties, Team, TypeScope, UnitTypeCore, Units};
use campfire_math::{Num, Vec3};
use campfire_mode::Mode;
use campfire_sim::{Position, StableId};
use campfire_stats::Life;
use campfire_vision::internals::SIGHTS;
use campfire_vision::{Sight, Vision};

use crate::fixtures::units::UnitsFixtures;

/// The test fixtures of `Vision` that need a crate above its own.
pub trait VisionFixtures {
    /// Holds on unit `id` the test tags whose effects make `properties`, as its modifiers
    /// would.
    fn set_properties(world: &mut World, id: StableId, properties: TagProperties);

    /// Loads the mode's seers, each of 100 life, so it may be a target, and seeing one of
    /// `SIGHTS`, named for it.
    fn load_seers(world: &mut World);

    /// A seer of `team` at `x`, `z`, seeing `sight`, one of `SIGHTS`.
    fn spawn_seer(world: &mut World, x: i64, z: i64, team: Team, sight: Option<i64>) -> StableId;
}

impl VisionFixtures for Vision {
    fn set_properties(world: &mut World, id: StableId, properties: TagProperties) {
        Units::hold_properties(world, id, properties);
    }

    fn load_seers(world: &mut World) {
        for sight in SIGHTS {
            let core = UnitTypeCore::default();
            let seer = Units::load_type(world, TypeScope::Mode, &Vision::seer(sight), &core);
            Units::give_type(world, seer, Life::full(Num::int(100)));
            if let Some(range) = sight {
                Units::give_type(world, seer, Sight::new(Num::int(range)).unwrap());
            }
        }
    }

    fn spawn_seer(world: &mut World, x: i64, z: i64, team: Team, sight: Option<i64>) -> StableId {
        let pos = Position::new(Vec3::new(Num::int(x), Num::ZERO, Num::int(z))).unwrap();
        Mode::spawn_named(world, &Vision::seer(sight), pos, team, ())
    }
}
