use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::entity::Entity;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::StableId;
use campfire_stats::{Grant, GrantFrom, Grants, Hold, ModifierBook, ModifierParams, Modifiers};
use campfire_values::{ModifierId, Rank};

use crate::inventory::Inventory;
use crate::item_book::ItemBook;

/// The grants of the modifiers each unit holds from the items it carries.
#[derive(Debug)]
pub struct ItemHolds;

impl ItemHolds {
    /// Pushes the grants of each unit's carried items: each item modifier under the items' hold
    /// from the unit itself at rank 1 while an item it carries holds it, once however many do, and
    /// none once it carries none. The hold is the items' own, so a passive of the same modifier
    /// neither keeps nor releases it. What a unit wants follows
    /// from its inventory, its modifiers and the books only, so a run visits only the units whose
    /// inventory or modifiers changed since its last, unless a book changed.
    pub(super) fn want_items(
        (items, book): (Res<'_, ItemBook>, Res<'_, ModifierBook>),
        params: ModifierParams<'_, '_>,
        mut grants: ResMut<'_, Grants>,
        units: Query<'_, '_, (Entity, &StableId, Ref<'_, Inventory>, Ref<'_, Modifiers>)>,
        mut held: Local<'_, Vec<ModifierId>>,
    ) {
        if items.modifiers().is_empty() {
            return;
        }
        let every = items.is_changed() || book.is_changed() || params.is_changed();
        for (entity, &id, inventory, modifiers) in &units {
            if !(every || inventory.is_changed() || modifiers.is_changed()) {
                continue;
            }
            held.clear();
            held.extend(inventory.slots().iter().flatten().flat_map(|carried| {
                let item = items
                    .get(carried.item)
                    .expect("a carried item is in the book");
                item.modifiers.iter().copied()
            }));
            held.sort_unstable();
            held.dedup();
            let mut run = grants.of(entity, id);
            for &modifier in items.modifiers() {
                let holds = held.binary_search(&modifier).is_ok();
                run.push(Grant {
                    modifier,
                    from: GrantFrom::Itself,
                    hold: Hold::Item,
                    action: None,
                    rank: holds.then_some(Rank::FIRST),
                });
            }
        }
    }
}
