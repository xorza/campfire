use bevy_ecs::world::World;
use campfire_common::Ticks;
use campfire_script::{Effect, Frame};
use campfire_units::{Lifespan, SpawnAt, Spawner};

/// A unit a listed `spawn` makes, `at` its place, of the acting unit's team and player, with a
/// timed life of `life` when it has one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ActionsEffect {
    pub(crate) at: SpawnAt,
    pub(crate) life: Option<Ticks>,
}

impl Effect for ActionsEffect {
    // The unit spawns with the id the call took, and lives `life` ticks from this one.
    fn apply(self, world: &mut World, frame: &mut Frame) {
        let now = frame.now();
        let spawner = world.non_send::<Spawner>().clone();
        let entity = spawner.spawn(world, self.at, ());
        if let Some(life) = self.life {
            world
                .entity_mut(entity)
                .insert(Lifespan::until(now.after(life)));
        }
    }
}
