use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;
use campfire_sim::Capability;

use crate::declare::callbacks::Callbacks;

/// What a cast that resolved spends of the unit of the entity besides its slot, the slot's:
/// items, a use of the consumable in it.
pub(crate) type Spend = fn(&mut World, Entity, u8);

/// What a resolved cast spends besides its slot's cooldown and charges, which the capability that
/// keeps more of a slot declares, so a cast names none of its types: items, as a use spends the
/// consumable in its slot.
#[derive(Resource, Debug, Default)]
pub(crate) struct CastSpends(Option<Spend>);

impl Callbacks for CastSpends {
    type Key = ();
    type Callback = Spend;

    fn owner((): ()) -> Option<Capability> {
        Some(Capability::Items)
    }

    fn register(&mut self, (): (), spend: Spend) {
        assert!(self.0.is_none(), "a cast's spend registers once");
        self.0 = Some(spend);
    }
}

impl CastSpends {
    /// Spends what the cast of the unit of `entity`, from `slot`, spends beside its slot.
    pub(super) fn spend(world: &mut World, entity: Entity, slot: u8) {
        if let Some(spend) = world.resource::<CastSpends>().0 {
            spend(world, entity, slot);
        }
    }
}
