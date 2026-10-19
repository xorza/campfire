use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use campfire_actions::{ActionKind, ActionsData, SlotClaim, SlotClaims};
use campfire_common::DeclaredName;
use campfire_declare::{BookCx, BookError, BookPlace, NamedMessages, Section, TypePlace};
use campfire_players::ResourceBook;
use campfire_sim::Capability;
use campfire_stats::PassiveData;
use campfire_values::PackageIndex;

use crate::Items;
use crate::error::ItemsDataError;
use crate::inventory_data::InventoryData;
use crate::item_data::ItemData;
use crate::items_data::ItemsData;
use crate::shop_data::ShopData;

impl Items {
    /// Adds the name of each item type of the mode, a message of the mode's package.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn named_messages(
        cx: &BookCx<'_>,
        messages: &mut NamedMessages,
    ) -> Result<(), BookError> {
        let items = cx.package(PackageIndex::MODE).content.get::<ItemsData>();
        for (id, item) in items.into_iter().flat_map(|items| &items.0) {
            let at = BookPlace::Entry {
                table: ItemsData::KEY,
                name: id.clone(),
            };
            messages.name(PackageIndex::MODE, at, &item.name);
        }
        Ok(())
    }

    /// Claims the slots of each inventory, of its slot kind, as its items fill them.
    #[expect(
        clippy::unnecessary_wraps,
        reason = "a book's builder and an extension take the build's signature"
    )]
    pub(crate) fn claim_slots(cx: &BookCx<'_>, claims: &mut SlotClaims) -> Result<(), BookError> {
        for unit in cx.types() {
            if let Some(inventory) = unit.data.section::<InventoryData>() {
                claims.0.push(SlotClaim {
                    unit_type: unit.unit_type,
                    kind: inventory.kind.clone(),
                    slots: NonZeroUsize::from(inventory.slots).get(),
                    by: Capability::Items,
                });
            }
        }
        Ok(())
    }

    /// The rules of the item types: only the mode holds them; none holds a modifier that is a
    /// passive of the mode's unit types or actions, or names an action of the `use` kind, which
    /// only its object runs; none is built from itself; and each shop sells only items that cost
    /// in the one resource it takes. A name the mode does not have, as a resource, is the books'
    /// problem.
    pub(crate) fn rule(cx: &BookCx<'_>) -> Result<(), BookError> {
        for (package, source) in cx.packages().skip(1) {
            if source.content.get::<ItemsData>().is_some() {
                let at = BookPlace::Section(ItemsData::KEY);
                return Err(BookError::rule(package, at, ItemsDataError::OutsideMode));
            }
        }
        let mode = cx.package(PackageIndex::MODE);
        let none = ItemsData::default();
        let items = &mode.content.get::<ItemsData>().unwrap_or(&none).0;
        let fail = |item: &DeclaredName, problem| {
            let at = BookPlace::Entry {
                table: ItemsData::KEY,
                name: item.clone(),
            };
            Err(BookError::rule(PackageIndex::MODE, at, problem))
        };
        let actions = mode.content.get::<ActionsData>().map(|actions| &actions.0);
        let unit_passives = mode.units.iter().filter_map(|(name, unit)| {
            let holder = BookPlace::UnitType(TypePlace::Declared(name.clone()));
            Some((holder, &unit.section::<PassiveData>()?.0))
        });
        let action_passives = actions.into_iter().flatten().filter_map(|(name, action)| {
            let holder = BookPlace::Entry {
                table: ActionsData::KEY,
                name: name.clone(),
            };
            Some((holder, action.passive_modifier.as_ref()?))
        });
        for (holder, modifier) in unit_passives.chain(action_passives) {
            let held = items
                .iter()
                .find(|(_, item)| item.modifiers.contains(modifier));
            if let Some((item, _)) = held {
                let modifier = modifier.clone();
                return fail(item, ItemsDataError::SharedPassive { modifier, holder });
            }
        }
        for (name, item) in items {
            let action = item.action.as_ref();
            let used = action.filter(|action| {
                let data = actions.and_then(|actions| actions.get(*action));
                data.is_some_and(|data| data.kind() == ActionKind::Use)
            });
            if let Some(action) = used {
                return fail(name, ItemsDataError::UseAction(action.clone()));
            }
        }
        if let Some(looped) = Self::component_loop(items) {
            return fail(looped, ItemsDataError::ComponentLoop);
        }
        let resources = cx.book_if::<ResourceBook>()?;
        for unit in cx.standing() {
            let Some(shop) = unit.data.section::<ShopData>() else {
                continue;
            };
            let taken = resources.and_then(|resources| resources.0.named(shop.resource.as_str()));
            if taken.is_none() {
                continue;
            }
            let declared = |resource: &DeclaredName| {
                resources.is_some_and(|resources| resources.0.named(resource.as_str()).is_some())
            };
            let other = |item: &&DeclaredName| {
                let costs = items
                    .get(*item)
                    .into_iter()
                    .flat_map(|item| item.cost.keys());
                let mut costs = costs.filter(|resource| declared(resource));
                costs.any(|resource| *resource != shop.resource)
            };
            if let Some(item) = shop.items.iter().find(other) {
                let at = BookPlace::UnitType(unit.place());
                let problem = ItemsDataError::ShopResource(item.clone());
                return Err(BookError::rule(unit.package, at, problem));
            }
        }
        Ok(())
    }

    /// An item of `items` built, through its components, from itself, if one is. The walk keeps
    /// its own stack, as a chain of components as long as the data holds would overflow the
    /// thread's.
    fn component_loop(items: &BTreeMap<DeclaredName, ItemData>) -> Option<&DeclaredName> {
        /// Where the walk is with an item: on the path, or done with every item it is built
        /// from; an item not met has none.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Mark {
            OnPath,
            Done,
        }
        let components = |id: &DeclaredName| {
            items
                .get(id)
                .map_or(&[][..], |item| item.components.as_slice())
        };
        let mut marks: BTreeMap<&DeclaredName, Mark> = BTreeMap::new();
        // Each item on the path, with the place of its next component to walk.
        let mut path: Vec<(&DeclaredName, usize)> = Vec::new();
        for root in items.keys() {
            if marks.contains_key(root) {
                continue;
            }
            marks.insert(root, Mark::OnPath);
            path.push((root, 0));
            while let Some((id, next)) = path.last_mut() {
                let Some(component) = components(id).get(*next) else {
                    marks.insert(*id, Mark::Done);
                    path.pop();
                    continue;
                };
                *next += 1;
                match marks.get(component) {
                    Some(Mark::OnPath) => return Some(component),
                    Some(Mark::Done) => {}
                    None => {
                        marks.insert(component, Mark::OnPath);
                        path.push((component, 0));
                    }
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use campfire_values::MessageId;

    use super::*;

    #[test]
    fn a_loop_is_found_however_long_the_chain_of_components() {
        // 100,000 items, each built from the next, a chain the walk takes on its own stack. With
        // no loop the walk finds none; once the last is built from the first, the walk from the
        // first meets the first again.
        let name = |at: usize| DeclaredName::new(&format!("i{at:06}")).unwrap();
        let item = |components: Vec<DeclaredName>| ItemData {
            name: MessageId::new("item").unwrap(),
            cost: BTreeMap::new(),
            components,
            stack: NonZeroU32::MIN,
            uses: None,
            modifiers: Vec::new(),
            action: None,
        };
        let count = 100_000;
        let mut items: BTreeMap<DeclaredName, ItemData> = (0..count)
            .map(|at| {
                let next = (at + 1 < count).then(|| name(at + 1));
                (name(at), item(next.into_iter().collect()))
            })
            .collect();
        assert_eq!(Items::component_loop(&items), None);
        items.get_mut(&name(count - 1)).unwrap().components = vec![name(0)];
        assert_eq!(Items::component_loop(&items), Some(&name(0)));
    }
}
