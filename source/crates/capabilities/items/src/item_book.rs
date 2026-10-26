use std::num::{NonZeroI64, NonZeroU32};
use std::sync::Arc;

use bevy_ecs::resource::Resource;
use campfire_actions::ActionBook;
use campfire_common::DeclaredName;
use campfire_declarations::{Book, BookCx, BookError, BookPlace, Section};
use campfire_players::ResourceAmount;
use campfire_script::{MatchNames, UnknownName};
use campfire_stats::ModifierBook;
use campfire_values::{
    ActionId, ItemId, ModifierId, NameId, Named, Names, PackageIndex, ResourceId,
};

use crate::Items;
use crate::error::ItemError;
use crate::items_data::ItemsData;

/// The mode's item types, by id: package data, not state. A full stack of each costs what an
/// amount holds, and each costs at least its components. A clone shares the items, as a write
/// that reads them holds them.
#[derive(Resource, Debug, Clone, PartialEq, Eq)]
#[component(immutable)]
pub struct ItemBook(Arc<ItemTables>);

/// The items of an `ItemBook`.
#[derive(Debug, Default, PartialEq, Eq)]
struct ItemTables {
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
    /// The book of `items`, named `names`, the ids following their order; an error for an item
    /// whose full stack costs more than an amount holds, or that costs less than its components.
    pub fn new(
        names: Names<ItemId>,
        items: impl IntoIterator<Item = ItemSpec>,
    ) -> Result<Self, ItemError> {
        Self::of(names, items.into_iter().collect())
    }

    fn of(names: Names<ItemId>, items: Vec<ItemSpec>) -> Result<Self, ItemError> {
        assert_eq!(names.len(), items.len(), "an item has a name");
        for (index, item) in items.iter().enumerate() {
            let id = ItemId::at(index);
            let stack = NonZeroI64::from(item.stack).get();
            for cost in &item.cost {
                debug_assert!(cost.amount >= 0, "an item costs no amount below 0");
                if cost.amount.checked_mul(stack).is_none() {
                    return Err(ItemError::StackCost(id));
                }
            }
            let cost_of = |component: &ItemId| &items[component.index()];
            let resources = item
                .components
                .iter()
                .flat_map(|component| &cost_of(component).cost);
            for resource in resources.map(|cost| cost.resource) {
                let parts = item.components.iter().try_fold(0_i64, |parts, component| {
                    parts.checked_add(cost_of(component).cost_in(resource))
                });
                if parts.is_none_or(|parts| item.cost_in(resource) < parts) {
                    return Err(ItemError::CheaperThanComponents(id));
                }
            }
        }
        let mut modifiers: Vec<ModifierId> = items
            .iter()
            .flat_map(|item| item.modifiers.iter().copied())
            .collect();
        modifiers.sort_unstable();
        modifiers.dedup();
        Ok(Self(Arc::new(ItemTables {
            names,
            specs: items,
            modifiers,
        })))
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
    const SCRIPTS_ONLY: bool = false;

    fn share(&self, _: &mut MatchNames) {}
}

impl ItemBook {
    /// The mode's item types, each with its cost in the players' resources, its components, its
    /// modifiers and its action, of the mode's package; an error for a name the mode does not
    /// have, or costs that break a rule of the book.
    pub fn build(cx: &BookCx<'_>) -> Result<Self, BookError> {
        let index = PackageIndex::MODE;
        let none = ItemsData::default();
        let items = cx
            .package(index)
            .content
            .get::<ItemsData>()
            .unwrap_or(&none);
        let ids: Names<ItemId> = items.0.keys().map(DeclaredName::as_str).collect();
        let no_resources = Names::default();
        let resources = Items::resources(cx, &no_resources)?;
        let modifiers = cx.book::<ModifierBook>()?;
        let actions = cx.book::<ActionBook>()?;
        let mut specs = Vec::with_capacity(items.0.len());
        for (name, item) in &items.0 {
            let spec = || {
                let cost = item.cost.iter().map(|(resource, &amount)| {
                    let found = resources.named(resource.as_str());
                    Ok(ResourceAmount {
                        resource: found
                            .ok_or_else(|| UnknownName::new(ResourceId::NAMES, resource))?,
                        amount: i64::from(amount),
                    })
                });
                let components = item.components.iter().map(|component| {
                    let found = ids.named(component.as_str());
                    found.ok_or_else(|| UnknownName::new(ItemId::NAMES, component))
                });
                let held = item.modifiers.iter().map(|modifier| {
                    let found = modifiers.names().named((index, modifier.as_str()));
                    found.ok_or_else(|| UnknownName::new(ModifierId::NAMES, modifier))
                });
                let action = item.action.as_ref().map(|action| {
                    let found = actions.names().named((index, action.as_str()));
                    found.ok_or_else(|| UnknownName::new(ActionId::NAMES, action))
                });
                Ok(ItemSpec {
                    cost: cost.collect::<Result<_, UnknownName>>()?,
                    components: components.collect::<Result<_, UnknownName>>()?,
                    stack: item.stack,
                    uses: item.uses,
                    modifiers: held.collect::<Result<_, UnknownName>>()?,
                    action: action.transpose()?,
                })
            };
            let at = BookPlace::Entry {
                table: ItemsData::KEY,
                name: name.clone(),
            };
            specs.push(spec().map_err(|name| BookError::unknown(index, at, name))?);
        }
        Self::new(ids, specs).map_err(|error| {
            let (ItemError::StackCost(item) | ItemError::CheaperThanComponents(item)) = error;
            let name = items
                .0
                .keys()
                .nth(item.index())
                .expect("an item of the package");
            let at = BookPlace::Entry {
                table: ItemsData::KEY,
                name: name.clone(),
            };
            BookError::rule(index, at, error)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An item that costs `costs`, amounts of resources 0 and 1, in stacks of `stack`, built
    /// from the items at `components`.
    fn item(costs: [i64; 2], stack: u32, components: &[usize]) -> ItemSpec {
        let resources = (0..).map(ResourceId::at).zip(costs);
        ItemSpec {
            cost: resources
                .filter(|&(_, amount)| amount != 0)
                .map(|(resource, amount)| ResourceAmount { resource, amount })
                .collect(),
            components: components.iter().copied().map(ItemId::at).collect(),
            stack: NonZeroU32::new(stack).unwrap(),
            uses: None,
            modifiers: Vec::new(),
            action: None,
        }
    }

    fn book(items: impl IntoIterator<Item = ItemSpec>) -> Result<ItemBook, ItemError> {
        let items: Vec<ItemSpec> = items.into_iter().collect();
        let names: Vec<String> = (0..items.len()).map(|at| format!("item{at}")).collect();
        ItemBook::new(names.iter().map(String::as_str).collect(), items)
    }

    #[test]
    fn a_book_bounds_a_full_stacks_cost_and_an_item_costs_its_components_at_least() {
        // A full stack: ⌊(2⁶³ − 1) / 2⌋ = 2⁶² − 1, by 2, is 2⁶³ − 2, which an amount holds; by 3
        // it is past 2⁶³ − 1. In resource 1 as in 0.
        let half = i64::MAX / 2;
        assert!(book([item([half, 0], 2, &[])]).is_ok());
        for over in [item([half, 0], 3, &[]), item([0, half], 3, &[])] {
            assert_eq!(book([over]), Err(ItemError::StackCost(ItemId::at(0))));
        }
        // Components of 350 gold, and of 400 gold and 25 wood, the second twice: 350 + 2 · 400 =
        // 1150 gold and 2 · 25 = 50 wood at least. One less of either, or no wood, is less.
        let parts = [item([350, 0], 1, &[]), item([400, 25], 1, &[])];
        let built = |costs| {
            parts
                .clone()
                .into_iter()
                .chain([item(costs, 1, &[0, 1, 1])])
        };
        assert!(book(built([1150, 50])).is_ok());
        let cheaper = Err(ItemError::CheaperThanComponents(ItemId::at(2)));
        for costs in [[1149, 50], [1150, 49], [1150, 0]] {
            assert_eq!(book(built(costs)), cheaper, "{costs:?}");
        }
        // Components whose costs sum past what an amount holds: no item costs that much.
        let dear = [item([half + 1, 0], 1, &[]), item([half + 1, 0], 1, &[])];
        let summed = dear.into_iter().chain([item([i64::MAX, 0], 1, &[0, 1])]);
        assert_eq!(
            book(summed),
            Err(ItemError::CheaperThanComponents(ItemId::at(2)))
        );
    }
}
