use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_declarations::{CallbackTable, Callbacks};
use campfire_sim::Capability;

/// What a cast that resolved spends of the unit of the entity besides its slot, the slot's:
/// items, a use of the consumable in it.
pub(crate) type Spend = fn(&mut World, Entity, u8);

/// What a resolved cast spends besides its slot's cooldown and charges, which each capability that
/// keeps more of a slot declares by itself, so a cast names none of its types: items, as a use
/// spends the consumable in its slot.
#[derive(Resource, Debug, Default)]
pub struct CastSpends(CallbackTable<Spend, { Capability::ALL.len() }>);

impl Callbacks for CastSpends {
    type Key = Capability;
    type Callback = Spend;

    fn capability(capability: Capability) -> Option<Capability> {
        Some(capability)
    }

    fn register(&mut self, capability: Capability, spend: Spend) {
        self.0.register(capability as usize, spend);
    }
}

impl CastSpends {
    /// Spends what the cast of the unit of `entity`, from `slot`, spends beside its slot, by each
    /// capability that keeps more of it.
    pub fn spend(world: &mut World, entity: Entity, slot: u8) {
        for capability in Capability::ALL {
            if let Some(spend) = world.resource::<Self>().0.get(capability as usize) {
                spend(world, entity, slot);
            }
        }
    }
}
