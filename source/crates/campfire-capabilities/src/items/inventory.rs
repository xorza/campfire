use std::num::{NonZeroU8, NonZeroU32};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::actions::action_slots::ActionSlots;
use crate::actions::slot_kind::SlotKind;
use crate::items::item_book::ItemBook;
use crate::items::item_id::ItemId;
use crate::players::resource_id::ResourceId;
use crate::state_types::data_kind::DataKind;
use crate::state_types::kinded::Kinded;
use crate::units::block::Block;

/// What a carrier carries: its inventory's slots, in order, each empty or holding a stack of one
/// item type, and the slot kind whose action slots they fill, one each. State of `items`.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    kind: SlotKind,
    slots: Vec<Option<ItemStack>>,
}

/// A stack in a slot: its item type, how many, at least one and at most the type's stack, and
/// the uses left of a consumable's item on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemStack {
    pub item: ItemId,
    pub count: NonZeroU32,
    pub uses: Option<NonZeroU32>,
}

impl Inventory {
    /// `slots` empty slots, which fill action slots of `kind`.
    pub(crate) fn new(slots: NonZeroU8, kind: SlotKind) -> Inventory {
        Inventory {
            kind,
            slots: vec![None; usize::from(slots.get())],
        }
    }

    /// The group a tag blocks of the action in a slot of `kind`, of a unit that carries
    /// `inventory`: `use` for an item's slot, whatever its action's kind, else `cast`.
    pub(crate) fn group(inventory: Option<&Inventory>, kind: SlotKind) -> Block {
        if inventory.is_some_and(|inventory| inventory.kind == kind) {
            Block::Use
        } else {
            Block::Cast
        }
    }

    pub const fn slots(&self) -> &[Option<ItemStack>] {
        self.slots.as_slice()
    }

    /// The price in `resource` of buying `item` of `book`: its cost less the costs of the
    /// components the inventory holds, each item it carries counted for one of them at most, by
    /// slot order, which `given_up` counts slot by slot; `None` with no room for the item once
    /// they left: no slot of its type with room in its stack, and no empty one.
    pub(crate) fn purchase(
        &self,
        book: &ItemBook,
        item: ItemId,
        resource: ResourceId,
        given_up: &mut Vec<u32>,
    ) -> Option<i64> {
        let spec = book
            .get(item)
            .expect("an item the shop sells is in the book");
        given_up.clear();
        given_up.resize(self.slots.len(), 0);
        let mut price = spec.cost_in(resource);
        for &component in &spec.components {
            let held = self
                .slots
                .iter()
                .zip(given_up.iter())
                .position(|(slot, &taken)| {
                    slot.is_some_and(|carried| {
                        carried.item == component && carried.count.get() > taken
                    })
                });
            if let Some(at) = held {
                given_up[at] += 1;
                let component = book.get(component).expect("a component is in the book");
                price -= component.cost_in(resource);
            }
        }
        self.place(item, spec.stack, given_up)?;
        Some(price)
    }

    /// Gives up the items `given_up` counts from each slot, then puts one `item` of `book` in: on
    /// a stack of its type with room, else in the first empty slot. The purchase found room for
    /// it.
    pub(crate) fn complete(&mut self, book: &ItemBook, item: ItemId, given_up: &[u32]) {
        let spec = book
            .get(item)
            .expect("an item the shop sells is in the book");
        for (slot, &taken) in self.slots.iter_mut().zip(given_up) {
            if let Some(carried) = slot {
                match NonZeroU32::new(carried.count.get() - taken) {
                    Some(count) => carried.count = count,
                    None => *slot = None,
                }
            }
        }
        let at = self
            .place(item, spec.stack, &[])
            .expect("the purchase found room");
        let slot = &mut self.slots[at];
        match slot {
            Some(carried) => {
                carried.count = carried
                    .count
                    .checked_add(1)
                    .expect("a stack below its most");
            }
            None => {
                *slot = Some(ItemStack {
                    item,
                    count: NonZeroU32::MIN,
                    uses: spec.uses,
                });
            }
        }
    }

    /// The slot one more `item`, of `stack` to a slot, goes in once the items `given_up` counts
    /// left, none past its end: the first of its type with room, else the first empty one, or one
    /// they empty.
    fn place(&self, item: ItemId, stack: NonZeroU32, given_up: &[u32]) -> Option<usize> {
        let mut empty = None;
        for (at, slot) in self.slots.iter().enumerate() {
            let taken = given_up.get(at).copied().unwrap_or(0);
            match slot.map(|carried| (carried.item, carried.count.get() - taken)) {
                Some((of, count)) if of == item && count > 0 && count < stack.get() => {
                    return Some(at);
                }
                Some((_, 0)) | None => {
                    empty.get_or_insert(at);
                }
                Some(_) => {}
            }
        }
        empty
    }

    /// Takes the stack in `slot` out; `None` for no such slot, or an empty one.
    pub(crate) fn take(&mut self, slot: u8) -> Option<ItemStack> {
        self.slots.get_mut(usize::from(slot))?.take()
    }

    /// Puts `carried` back in `slot`, which a take emptied.
    pub(crate) fn restore(&mut self, slot: u8, carried: ItemStack) {
        let slot = &mut self.slots[usize::from(slot)];
        debug_assert!(slot.is_none(), "a slot a take emptied");
        *slot = Some(carried);
    }

    /// Notes in `items` each slot's item type, none for an empty slot, for `follow` to compare.
    pub(crate) fn note(&self, items: &mut Vec<Option<ItemId>>) {
        items.clear();
        items.extend(
            self.slots
                .iter()
                .map(|slot| slot.map(|carried| carried.item)),
        );
    }

    /// Fills each action slot of a slot whose item type differs from `before`'s with its item's
    /// action, or none, afresh, in `slots`, the carrier's.
    pub(crate) fn follow(
        &self,
        book: &ItemBook,
        slots: &mut ActionSlots,
        before: &[Option<ItemId>],
    ) {
        let first = slots.first_of(self.kind);
        for (at, (slot, &was)) in self.slots.iter().zip(before).enumerate() {
            let item = slot.map(|carried| carried.item);
            if item != was {
                let action =
                    item.and_then(|item| book.get(item).expect("an item of the book").action);
                let at = u8::try_from(at).expect("a slot index fits u8");
                slots.fill(first + at, action);
            }
        }
    }

    /// Spends a use of the consumable in the item's slot whose action slot is `slot` of `slots`,
    /// the carrier's, when `slot` is one of its inventory's: the item on top goes with its last
    /// use, the next fresh, and the inventory slot with the last item, its action slot then empty.
    pub(crate) fn spend_use(&mut self, book: &ItemBook, slots: &mut ActionSlots, slot: u8) {
        let first = slots.first_of(self.kind);
        let Some(at) = slot
            .checked_sub(first)
            .filter(|&at| usize::from(at) < self.slots.len())
        else {
            return;
        };
        let held = &mut self.slots[usize::from(at)];
        let Some(carried) = held else {
            return;
        };
        let Some(uses) = carried.uses else {
            return;
        };
        if let Some(left) = NonZeroU32::new(uses.get() - 1) {
            carried.uses = Some(left);
            return;
        }
        if let Some(count) = NonZeroU32::new(carried.count.get() - 1) {
            carried.count = count;
            carried.uses = book.get(carried.item).expect("an item of the book").uses;
        } else {
            *held = None;
            slots.fill(slot, None);
        }
    }

    /// Swaps slots `from` and `to`, empty or not, and their action slots in `slots`, the
    /// carrier's, each keeping its cooldown; nothing when either is no slot.
    pub(crate) fn swap_with(&mut self, from: u8, to: u8, slots: Option<&mut ActionSlots>) {
        if usize::from(from.max(to)) >= self.slots.len() {
            return;
        }
        self.slots.swap(usize::from(from), usize::from(to));
        if let Some(slots) = slots {
            let first = slots.first_of(self.kind);
            slots.swap(first + from, first + to);
        }
    }
}

impl SimComponent for Inventory {
    const NAME: &'static str = "items.inventory";

    // Each stack is of an item type the book holds, no larger than its type's, and with uses left
    // only for a consumable, no more than its type's.
    fn check(&self, world: &World, _: Entity) -> bool {
        let book = world.resource::<ItemBook>();
        self.slots.iter().flatten().all(|carried| {
            book.get(carried.item).is_some_and(|spec| {
                let uses = match (carried.uses, spec.uses) {
                    (Some(left), Some(most)) => left <= most,
                    (left, most) => left.is_none() && most.is_none(),
                };
                carried.count <= spec.stack && uses
            })
        })
    }
}

impl Kinded for Inventory {
    const KIND: DataKind = DataKind::Inventory;
}
