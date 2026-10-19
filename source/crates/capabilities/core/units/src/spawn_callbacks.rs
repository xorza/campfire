use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declare::{CallbackTable, Callbacks};
use campfire_sim::Capability;

/// What a capability above the core does to a unit the mode's spawner spawned, once its parts are
/// on it, in the capabilities' order: stats' passive of its type, navigation's room for a box.
pub(crate) type SpawnCallback = fn(&mut World, Entity);

/// The spawn callbacks of the capabilities above the core, by capability: each declares its own, so
/// the spawner names none, and a match runs only its capabilities' (02-engine-core.md, Spawn
/// reactions run on the spawner's path). Not state.
#[derive(Resource, Debug, Default)]
pub struct SpawnCallbacks(CallbackTable<SpawnCallback, { Capability::ALL.len() }>);

impl Callbacks for SpawnCallbacks {
    type Key = Capability;
    type Callback = SpawnCallback;

    fn capability(capability: Capability) -> Option<Capability> {
        Some(capability)
    }

    fn register(&mut self, capability: Capability, hook: SpawnCallback) {
        self.0.register(capability as usize, hook);
    }
}

impl SpawnCallbacks {
    /// Runs each capability's hook on the unit of `entity`, which the mode's spawner spawned, in
    /// the capabilities' order.
    pub fn run(world: &mut World, entity: Entity) {
        let hooks = world.resource::<Self>().0;
        for hook in hooks.iter() {
            hook(world, entity);
        }
    }
}
