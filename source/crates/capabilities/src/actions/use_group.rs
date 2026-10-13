use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::{Capability, SimComponent};
use serde::{Deserialize, Serialize};

use crate::actions::slot_kind::SlotKind;
use crate::actions::slot_kind_book::SlotKindBook;
use crate::declare::per_type::PerType;
use crate::state_types::data_kind::DataKind;
use crate::state_types::replication::Replication;
use crate::state_types::sending::SentOnce;
use crate::units::block::Block;
use crate::units::unit_entity::UnitEntity;

/// The kind of slot whose actions the unit uses rather than casts: its inventory's, which `items`
/// puts on with the inventory.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(transparent)]
pub struct UseGroup(SlotKind);

impl UseGroup {
    pub(crate) const fn new(kind: SlotKind) -> UseGroup {
        UseGroup(kind)
    }

    pub(crate) const fn kind(self) -> SlotKind {
        self.0
    }

    /// The group a tag blocks of the action in a slot of `kind`, of a unit with `used`: `use` for
    /// a slot of its use group, whatever its action's kind, else `cast`.
    pub(crate) fn group(used: Option<&UseGroup>, kind: SlotKind) -> Block {
        if used.is_some_and(|used| used.0 == kind) {
            Block::Use
        } else {
            Block::Cast
        }
    }
}

impl SimComponent for UseGroup {
    const NAME: &'static str = "actions.use_group";
    type Host = UnitEntity;

    // A kind the mode lacks would be read past the mode's kinds.
    fn check(&self, world: &World, _: Entity) -> bool {
        world.resource::<SlotKindBook>().0.holds(self.0, 0)
    }
}

impl Replication for UseGroup {
    const KIND: DataKind = DataKind::Inventory;
    type Sending = SentOnce;
}

impl PerType for UseGroup {
    const OWNER: Option<Capability> = Some(Capability::Items);
    const SCRIPTS_ONLY: bool = false;
}
