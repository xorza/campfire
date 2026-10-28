use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::entity_disabling::Disabled;
use bevy_ecs::lifecycle::HookContext;
use bevy_ecs::world::{DeferredWorld, World};
use campfire_declarations::ById;
use campfire_sim::{EntityIndex, SimComponent, StableId};
use campfire_states::{Replication, SentOnInsert};
use campfire_units::UnitEntity;
use campfire_values::UnitType;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::enter_spec::EnterSpec;

/// Marks a rider, out of the world inside its container, by the container's stable id
/// (transports.md, A rider leaves the world and keeps its entity): its hooks give it Bevy's
/// `Disabled` and take it back, so every query that does not name `Disabled` passes over it,
/// in a match and on each client that receives it, while its parts and its stable id stay.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable, on_insert = Self::leave_world, on_remove = Self::come_back)]
#[serde(transparent)]
pub struct Riding(StableId);

impl Riding {
    pub const fn new(container: StableId) -> Self {
        Self(container)
    }

    /// The container it rides in.
    pub const fn container(self) -> StableId {
        self.0
    }

    fn leave_world(mut world: DeferredWorld<'_>, context: HookContext) {
        world.commands().entity(context.entity).insert(Disabled);
    }

    /// A rider that leaves, or is despawned, comes back; a despawned one is gone by then.
    fn come_back(mut world: DeferredWorld<'_>, context: HookContext) {
        world
            .commands()
            .entity(context.entity)
            .try_remove::<Disabled>();
    }
}

impl SimComponent for Riding {
    const NAME: &'static str = "interaction.riding";
    type Host = UnitEntity;

    // A rider rides in a container the match holds, of a type that takes riders; that the
    // container's riders hold it, `Interaction` checks.
    fn check(&self, world: &World, _: Entity) -> bool {
        let container = world.resource::<EntityIndex>().get(self.0);
        let unit_type = container.and_then(|container| world.get::<UnitType>(container));
        let specs = world.resource::<ById<UnitType, EnterSpec>>();
        unit_type.is_some_and(|&unit_type| specs.get(unit_type).is_some())
    }
}

impl Replication for Riding {
    const KIND: DataKind = DataKind::Unit;
    type Sending = SentOnInsert;
}
