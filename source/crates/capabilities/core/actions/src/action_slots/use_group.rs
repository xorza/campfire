use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_declare::PerType;
use campfire_sim::SimComponent;
use campfire_state_types::{Replication, SentOnce};
use campfire_units::UnitEntity;
use campfire_values::Block;
use campfire_visibility::DataKind;
use serde::{Deserialize, Serialize};

use crate::action_book::ActionBook;
use crate::action_kind::ActionKind;
use crate::action_slots::ActionSlot;
use crate::slot_kind::SlotKind;
use crate::slot_kind_book::SlotKindBook;

/// The kind of slot whose actions the unit uses rather than casts: its inventory's, which `items`
/// puts on with the inventory.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[component(immutable)]
#[serde(transparent)]
pub struct UseGroup(SlotKind);

impl UseGroup {
    pub const fn new(kind: SlotKind) -> Self {
        Self(kind)
    }

    pub const fn kind(self) -> SlotKind {
        self.0
    }

    /// The group a tag blocks of the action in `slot`, of a unit with `used`, as `book` gives its
    /// kind: `use` for a slot of its use group, whatever its action's kind, else its kind's block,
    /// and `cast` for a slot that holds no action.
    pub fn group(used: Option<&Self>, slot: ActionSlot, book: &ActionBook) -> Block {
        if used.is_some_and(|used| used.0 == slot.kind) {
            return Block::Use;
        }
        let kind = slot
            .action
            .and_then(|id| book.get(id))
            .map(|action| action.kind);
        kind.map_or(Block::Cast, ActionKind::block)
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
    const SCRIPTS_ONLY: bool = false;
}
