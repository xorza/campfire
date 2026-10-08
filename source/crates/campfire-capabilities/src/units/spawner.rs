use std::fmt;
use std::rc::Rc;

use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_common::PlayerSlot;

use crate::units::spawn_at::SpawnAt;

/// How the match spawns a unit of a type, with the parts its type's kit gives, owned by a player
/// or none: the mode registers it as it installs, as the kits are its own, and a capability
/// that spawns a typed unit, such as production, calls it. Package data, not state.
#[derive(Clone)]
pub(crate) struct Spawner(Rc<SpawnFn>);

type SpawnFn = dyn Fn(&mut World, SpawnAt, Option<PlayerSlot>) -> Entity;

impl Spawner {
    pub(crate) fn new(
        spawn: impl Fn(&mut World, SpawnAt, Option<PlayerSlot>) -> Entity + 'static,
    ) -> Spawner {
        Spawner(Rc::new(spawn))
    }

    /// Spawns `at` in `world`, owned by `owner` when it names a player.
    pub(crate) fn spawn(
        &self,
        world: &mut World,
        at: SpawnAt,
        owner: Option<PlayerSlot>,
    ) -> Entity {
        (self.0)(world, at, owner)
    }
}

impl fmt::Debug for Spawner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Spawner")
    }
}
