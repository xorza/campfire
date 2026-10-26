use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::StableId;
use campfire_units::internals::Spawn;
use campfire_units::{Life, UnitTypeCore, Units};
use campfire_values::{Team, TypeScope};
use campfire_vision::internals::SIGHTS;
use campfire_vision::{Sight, Vision};

/// The test fixtures of `Vision` that need a crate above its own.
pub trait VisionFixtures {
    /// Loads the mode's seers, each of 100 life, so it may be a target, and seeing one of
    /// `SIGHTS`, named for it.
    fn load_seers(world: &mut World);

    /// A seer of `team` at `x`, `z`, seeing `sight`, one of `SIGHTS`.
    fn spawn_seer(world: &mut World, x: i64, z: i64, team: Team, sight: Option<i64>) -> StableId;
}

impl VisionFixtures for Vision {
    fn load_seers(world: &mut World) {
        for sight in SIGHTS {
            let core = UnitTypeCore::default();
            let seer = Units::load_type(world, TypeScope::Mode, &Self::seer(sight), &core);
            Units::give_type(world, seer, Life::full(Num::int(100)));
            if let Some(range) = sight {
                Units::give_type(world, seer, Sight::new(Num::int(range)).unwrap());
            }
        }
    }

    fn spawn_seer(world: &mut World, x: i64, z: i64, team: Team, sight: Option<i64>) -> StableId {
        let position = Vec3::new(Num::int(x), Num::ZERO, Num::int(z));
        Spawn::named(world, &Self::seer(sight), position)
            .team(team)
            .place(world)
    }
}
