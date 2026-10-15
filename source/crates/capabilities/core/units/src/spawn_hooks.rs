use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declare::Callbacks;
use campfire_sim::Capability;

/// What a capability above the core does to a unit the mode's spawner spawned, once its parts
/// and its type's passive are on it: navigation's room for a box.
pub(crate) type SpawnHook = fn(&mut World, Entity);

/// The spawn hooks of the capabilities above the core, by capability: each declares its own, so
/// the spawner names none, and a match runs only its capabilities'. Not state.
#[derive(Resource, Debug, Default)]
pub struct SpawnHooks([Option<SpawnHook>; Capability::ALL.len()]);

impl Callbacks for SpawnHooks {
    type Key = Capability;
    type Callback = SpawnHook;

    fn owner(capability: Capability) -> Option<Capability> {
        Some(capability)
    }

    fn register(&mut self, capability: Capability, hook: SpawnHook) {
        let slot = &mut self.0[capability as usize];
        assert!(
            slot.is_none(),
            "{capability:?} registers its spawn hook once"
        );
        *slot = Some(hook);
    }
}

impl SpawnHooks {
    /// Runs each capability's hook on the unit of `entity`, which the mode's spawner spawned, in
    /// the capabilities' order.
    pub fn run(world: &mut World, entity: Entity) {
        let hooks = world.resource::<SpawnHooks>().0;
        for hook in hooks.into_iter().flatten() {
            hook(world, entity);
        }
    }
}
