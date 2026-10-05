use std::num::{NonZeroU8, NonZeroU32};

use bevy_ecs::component::Component;
use bevy_ecs::entity::Entity;
use bevy_ecs::world::World;
use campfire_sim::SimComponent;
use serde::{Deserialize, Serialize};

use crate::items::item_book::ItemBook;
use crate::items::item_id::ItemId;
use crate::players::resource_id::ResourceId;

/// What a carrier carries: its inventory's slots, in order, each empty or holding a stack of one
/// item type. State of `items`.
#[derive(Component, Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventory {
    slots: Vec<Option<Carried>>,
}

/// A stack in a slot: its item type, how many, at least one and at most the type's stack, and
/// the uses left of a consumable's item on top.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carried {
    pub item: ItemId,
    pub count: NonZeroU32,
    pub uses: Option<NonZeroU32>,
}

impl Inventory {
    /// `slots` empty slots.
    pub(crate) fn new(slots: NonZeroU8) -> Inventory {
        Inventory {
            slots: vec![None; usize::from(slots.get())],
        }
    }

    pub fn slots(&self) -> &[Option<Carried>] {
        &self.slots
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
                *slot = Some(Carried {
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
    pub(crate) fn take(&mut self, slot: u8) -> Option<Carried> {
        self.slots.get_mut(usize::from(slot))?.take()
    }

    /// Puts `carried` back in `slot`, which a take emptied.
    pub(crate) fn restore(&mut self, slot: u8, carried: Carried) {
        let slot = &mut self.slots[usize::from(slot)];
        debug_assert!(slot.is_none(), "a slot a take emptied");
        *slot = Some(carried);
    }

    /// Swaps two slots, empty or not; `false` when either is no slot.
    pub(crate) fn swap(&mut self, from: u8, to: u8) -> bool {
        let (from, to) = (usize::from(from), usize::from(to));
        if from.max(to) >= self.slots.len() {
            return false;
        }
        self.slots.swap(from, to);
        true
    }
}

impl SimComponent for Inventory {
    const NAME: &'static str = "items.inventory";

    // Each stack is of an item type the book holds, and no larger than its type's.
    fn check(&self, world: &World, _: Entity) -> bool {
        let book = world.resource::<ItemBook>();
        self.slots.iter().flatten().all(|carried| {
            book.get(carried.item)
                .is_some_and(|spec| carried.count <= spec.stack)
        })
    }
}
