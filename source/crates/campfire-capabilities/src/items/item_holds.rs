use bevy_ecs::change_detection::{DetectChanges, Ref};
use bevy_ecs::system::{Local, Query, Res};
use campfire_sim::{SimTick, StableId, TickRate};

use crate::items::inventory::Inventory;
use crate::items::item_book::ItemBook;
use crate::stats::applier::Applier;
use crate::stats::carried_mut::CarriedMut;
use crate::stats::lifetime::Hold;
use crate::stats::modifier_book::ModifierBook;
use crate::stats::modifier_clocks::ModifierClocks;
use crate::stats::modifier_spec::ParamPlace;
use crate::stats::modifiers::Modifiers;
use crate::stats::param_book::ParamBook;
use crate::stats::param_sources::ParamSources;
use crate::units::modifier_id::ModifierId;
use crate::values::rank::Rank;

/// The modifiers each unit holds from the items it carries.
#[derive(Debug)]
pub(super) struct ItemHolds;

impl ItemHolds {
    /// Keeps the modifiers of each unit's carried items, as passives from the unit itself at rank
    /// 1, each once however many items hold it, and releases those of items it carries no more. No
    /// unit type's or action's passive is an item's modifier, which the load checks, so every
    /// passive of an item's modifier is an item's. The modifiers' params are the match's param
    /// book's. What a unit holds follows from its inventory, its modifiers and the books only, so a
    /// run visits only the units whose inventory or modifiers changed since its last, unless a book
    /// changed.
    pub(super) fn hold_items(
        (items, book): (Res<'_, ItemBook>, Res<'_, ModifierBook>),
        (tick, rate, params): (Res<'_, SimTick>, Res<'_, TickRate>, Res<'_, ParamBook>),
        sources: ParamSources<'_, '_>,
        mut units: Query<
            '_,
            '_,
            (
                &StableId,
                Ref<'_, Inventory>,
                &mut Modifiers,
                &mut ModifierClocks,
            ),
        >,
        mut held: Local<'_, Vec<ModifierId>>,
    ) {
        if items.modifiers().is_empty() {
            return;
        }
        let every = items.is_changed() || book.is_changed() || params.is_changed();
        let now = tick.start();
        for (&id, inventory, modifiers, clocks) in &mut units {
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
            let mut carried = CarriedMut::new(modifiers, clocks);
            for &modifier in items.modifiers() {
                let holds = held.binary_search(&modifier).is_ok();
                let holding = carried
                    .modifiers()
                    .get(modifier, Some(id))
                    .is_some_and(|instance| instance.lifetime.held_by(Hold::Passive));
                if holding && !holds {
                    carried.release(modifier, Some(id), Hold::Passive);
                } else if holds && !holding {
                    let applier = Applier {
                        source: Some(id),
                        ability: None,
                        rank: Rank::FIRST,
                        hold: Some(Hold::Passive),
                    };
                    let source = sources.get(id);
                    let param = |place: &ParamPlace| {
                        params.modifier_param(modifier, None, Rank::FIRST, place, source.as_ref())
                    };
                    carried.apply(book.application(modifier, applier, None, now, *rate, param));
                }
            }
        }
    }
}
