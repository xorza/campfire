use std::num::NonZeroU32;
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_core::{ActionId, Book, ModifierId, ResourceAmount, ResourceId, View};
use campfire_sim::Capability;
use campfire_values::Names;

use crate::item_id::ItemId;

/// The mode's item types, by id, as its package load checked them: package data, not state. A
/// clone shares the items, as a write that reads them holds them.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct ItemBook(Arc<Items>);

/// The items of an `ItemBook`.
#[derive(Debug, Default, PartialEq, Eq)]
struct Items {
    /// The items' names, by id.
    names: Names<ItemId>,
    specs: Vec<ItemSpec>,
    /// Every modifier an item holds, each once, in order.
    modifiers: Vec<ModifierId>,
}

/// An item type as a match reads it, its names resolved: its cost, the items it is built from,
/// how many share a slot, a consumable's uses, the modifiers it holds on its carrier, and its
/// active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemSpec {
    pub cost: Vec<ResourceAmount>,
    pub components: Vec<ItemId>,
    pub stack: NonZeroU32,
    pub uses: Option<NonZeroU32>,
    pub modifiers: Vec<ModifierId>,
    pub action: Option<ActionId>,
}

impl ItemBook {
    /// The book of `items`, named `names`, the ids following their order.
    pub fn new(names: Names<ItemId>, items: impl IntoIterator<Item = ItemSpec>) -> ItemBook {
        let items: Vec<ItemSpec> = items.into_iter().collect();
        assert_eq!(names.len(), items.len(), "an item has a name");
        let mut modifiers: Vec<ModifierId> = items
            .iter()
            .flat_map(|item| item.modifiers.iter().copied())
            .collect();
        modifiers.sort_unstable();
        modifiers.dedup();
        ItemBook(Arc::new(Items {
            names,
            specs: items,
            modifiers,
        }))
    }

    /// The items' names, by id.
    pub fn names(&self) -> &Names<ItemId> {
        &self.0.names
    }

    /// Every modifier an item holds, each once, in order.
    pub(crate) fn modifiers(&self) -> &[ModifierId] {
        self.0.modifiers.as_slice()
    }

    pub fn get(&self, item: ItemId) -> Option<&ItemSpec> {
        self.0.specs.get(item.index())
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

impl Book for ItemBook {
    const OWNER: Option<Capability> = Some(Capability::Items);
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &View) {}
}
