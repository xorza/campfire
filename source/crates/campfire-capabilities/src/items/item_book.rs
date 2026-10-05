use std::num::NonZeroU32;

use bevy_ecs::resource::Resource;

use crate::items::item_id::ItemId;
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::units::action_id::ActionId;
use crate::units::modifier_id::ModifierId;

/// The mode's item types, by id, as its package load checked them: package data, not state.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ItemBook {
    items: Vec<ItemSpec>,
    /// Every modifier an item holds, each once, in order.
    modifiers: Vec<ModifierId>,
}

/// An item type as a match reads it, its names resolved: its cost, the items it is built from,
/// how many share a slot, a consumable's uses, the modifiers it holds on its carrier, and its
/// active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ItemSpec {
    pub(crate) cost: Vec<ResourceAmount>,
    pub(crate) components: Vec<ItemId>,
    pub(crate) stack: NonZeroU32,
    pub(crate) uses: Option<NonZeroU32>,
    pub(crate) modifiers: Vec<ModifierId>,
    pub(crate) action: Option<ActionId>,
}

impl ItemBook {
    /// The book of `items`, the ids following their order.
    pub(crate) fn new(items: Vec<ItemSpec>) -> ItemBook {
        let mut modifiers: Vec<ModifierId> = items
            .iter()
            .flat_map(|item| item.modifiers.iter().copied())
            .collect();
        modifiers.sort_unstable();
        modifiers.dedup();
        ItemBook { items, modifiers }
    }

    /// Every modifier an item holds, each once, in order.
    pub(crate) fn modifiers(&self) -> &[ModifierId] {
        &self.modifiers
    }

    pub(crate) fn get(&self, item: ItemId) -> Option<&ItemSpec> {
        self.items.get(item.index())
    }
}

impl ItemSpec {
    /// What it costs in `resource`, 0 when it costs none.
    pub(crate) fn cost_in(&self, resource: ResourceId) -> i64 {
        self.cost
            .iter()
            .find(|cost| cost.resource == resource)
            .map_or(0, |cost| cost.amount)
    }
}
