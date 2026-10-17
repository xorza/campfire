use campfire_actions::{ActionBook, ActionSlots, SlotKindBook, UseGroup};
use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, ByType, Section};
use campfire_players::{ResourceAmount, ResourceBook};
use campfire_script::{Filter, NameKind, UnknownName};
use campfire_stats::ModifierBook;
use campfire_units::UnitTypes;
use campfire_values::{NameId, Names, PackageIndex, ResourceId};

use crate::Items;
use crate::error::ItemError;
use crate::inventory::Inventory;
use crate::inventory_data::InventoryData;
use crate::item_book::{ItemBook, ItemSpec};
use crate::item_id::ItemId;
use crate::items_data::ItemsData;
use crate::shop::Shop;
use crate::shop_data::ShopData;

impl Items {
    /// Adds the empty slots of each inventory to its unit type's slots, of its slot kind.
    pub(crate) fn inventory_slots(
        cx: &BookCx<'_>,
        slots: &mut ByType<ActionSlots>,
    ) -> Result<(), BookError> {
        let inventories = cx.book::<ByType<Inventory>>()?;
        let groups = cx.book::<ByType<UseGroup>>()?;
        for unit in cx.standing() {
            let (Some(inventory), Some(&group)) =
                (inventories.get(unit.unit_type), groups.get(unit.unit_type))
            else {
                continue;
            };
            let held = slots.take(unit.unit_type);
            let held = held.unwrap_or_else(|| ActionSlots::new([]));
            slots.set(unit.unit_type, inventory.extend_slots(held, group));
        }
        Ok(())
    }

    /// The names of the players' resources, none in a match without them.
    fn resources<'c>(
        cx: &'c BookCx<'_>,
        none: &'c Names<ResourceId>,
    ) -> Result<&'c Names<ResourceId>, BookError> {
        let resources = cx.book_if::<ResourceBook>()?;
        Ok(resources.map_or(none, |resources| &*resources.0))
    }
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
                            .ok_or_else(|| UnknownName::new(NameKind::Resource, resource))?,
                        amount: i64::from(amount),
                    })
                });
                let components = item.components.iter().map(|component| {
                    let found = ids.named(component.as_str());
                    found.ok_or_else(|| UnknownName::new(NameKind::Item, component))
                });
                let held = item.modifiers.iter().map(|modifier| {
                    let found = modifiers.names().named((index, modifier.as_str()));
                    found.ok_or_else(|| UnknownName::new(NameKind::Modifier, modifier))
                });
                let action = item.action.as_ref().map(|action| {
                    let found = actions.names().named((index, action.as_str()));
                    found.ok_or_else(|| UnknownName::new(NameKind::Ability, action))
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

impl Shop {
    /// The shop of each unit type that stands and sells, of the mode's item types and resources;
    /// an error for a name the mode does not have.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let items = cx.book::<ItemBook>()?.names();
        let no_resources = Names::default();
        let resources = Items::resources(cx, &no_resources)?;
        let tags = cx.fact::<UnitTypes>()?.tags();
        let mut book = ByType::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<ShopData>() else {
                continue;
            };
            let shop = || {
                let sells = data.items.iter().map(|name| {
                    let found = items.named(name.as_str());
                    found.ok_or_else(|| UnknownName::new(NameKind::Item, name))
                });
                let sells = sells.collect::<Result<_, UnknownName>>()?;
                let resource = resources.named(data.resource.as_str());
                let resource =
                    resource.ok_or_else(|| UnknownName::new(NameKind::Resource, &data.resource))?;
                let buyers = Filter::resolve(&data.buyers, tags)?;
                Ok(Self::new(
                    sells,
                    resource,
                    data.reach,
                    buyers,
                    data.sell_share,
                ))
            };
            let at = || BookPlace::UnitType(unit.place());
            let shop = shop().map_err(|name| BookError::unknown(unit.package, at(), name))?;
            book.set(unit.unit_type, shop);
        }
        Ok(book)
    }
}

impl Inventory {
    /// The inventory of each unit type that stands with one.
    pub fn book(cx: &BookCx<'_>) -> Result<ByType<Self>, BookError> {
        let mut book = ByType::default();
        for unit in cx.standing() {
            if let Some(data) = unit.data.section::<InventoryData>() {
                book.set(unit.unit_type, Self::new(data.slots));
            }
        }
        Ok(book)
    }

    /// The slot kind each unit type's inventory fills; an error for a kind the mode does not
    /// declare.
    pub fn groups(cx: &BookCx<'_>) -> Result<ByType<UseGroup>, BookError> {
        let kinds = &cx.book::<SlotKindBook>()?.0;
        let mut book = ByType::default();
        for unit in cx.standing() {
            let Some(data) = unit.data.section::<InventoryData>() else {
                continue;
            };
            let kind = kinds.names().named(data.kind.as_str()).ok_or_else(|| {
                let at = BookPlace::UnitType(unit.place());
                let name = UnknownName::new(NameKind::SlotKind, &data.kind);
                BookError::unknown(unit.package, at, name)
            })?;
            book.set(unit.unit_type, UseGroup::new(kind));
        }
        Ok(book)
    }
}
