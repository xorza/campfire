use bevy_ecs::entity::Entity;
use bevy_ecs::resource::Resource;
use bevy_ecs::world::World;

/// What a cast that resolved spends of the unit of the entity besides its slot, the slot's:
/// items, a use of the consumable in it.
pub(crate) type Spend = fn(&mut World, Entity, u8);

/// What a resolved cast spends besides its slot's cooldown and charges, which a capability that
/// keeps more of a slot registers as it installs, so a cast names none of its types. Not state.
#[derive(Resource, Debug, Default)]
pub(crate) struct CastSpends(Option<Spend>);

impl CastSpends {
    pub(crate) fn register(&mut self, spend: Spend) {
        assert!(self.0.is_none(), "a cast's spend registers once");
        self.0 = Some(spend);
    }

    /// Spends what the cast of the unit of `entity`, from `slot`, spends beside its slot.
    pub(crate) fn spend(world: &mut World, entity: Entity, slot: u8) {
        if let Some(spend) = world
            .get_resource::<CastSpends>()
            .and_then(|spends| spends.0)
        {
            spend(world, entity, slot);
        }
    }
}
