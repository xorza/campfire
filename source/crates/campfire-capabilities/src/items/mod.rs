use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Local, Query, Res};
use bevy_ecs::world::World;
use campfire_sim::{SimSet, SimTick, StableId, StateRegistry, TickRate};

use crate::actions::ActionsSet;
use crate::combat::CombatSet;
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

pub(crate) mod inventory;
pub(crate) mod inventory_data;
pub(crate) mod item_book;
pub(crate) mod item_data;
pub(crate) mod item_id;
pub(crate) mod items_api;
pub(crate) mod shop;
pub(crate) mod shop_data;

/// The `items` capability: item types a mode package declares, the inventories its unit types
/// carry them in, and its shop.
#[derive(Debug)]
pub struct Items;

/// The systems of `items`, for the capabilities built on it to order theirs against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ItemsSet {
    /// In `SimSet::Inputs`, once the tick's trades are in: carried items hold their modifiers.
    HoldAtInputs,
}

impl Items {
    /// Adds items to a match, on stats: an empty item book until the mode's books fill it, the
    /// inventories its units carry, as state, and the modifiers their items hold, as each tick
    /// starts once the trades are in, and in Resolve once a use may have spent an item.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.insert_resource(ItemBook::default());
        registry.register_component::<Inventory>();
        schedule.add_systems((
            hold_items
                .in_set(SimSet::Inputs)
                .in_set(ItemsSet::HoldAtInputs)
                .before(ActionsSet::HoldAtInputs),
            hold_items
                .in_set(SimSet::Resolve)
                .after(CombatSet::Damage)
                .before(ActionsSet::HoldAtResolve),
        ));
    }
}

/// Keeps the modifiers of each unit's carried items, as passives from the unit itself at rank 1,
/// each once however many items hold it, and releases those of items it carries no more. No unit
/// type's or action's passive is an item's modifier, which the load checks, so every passive of
/// an item's modifier is an item's. The modifiers' params are the match's param book's.
fn hold_items(
    (items, book): (Option<Res<'_, ItemBook>>, Option<Res<'_, ModifierBook>>),
    (tick, rate, params): (Res<'_, SimTick>, Res<'_, TickRate>, Res<'_, ParamBook>),
    sources: ParamSources<'_, '_>,
    mut units: Query<'_, '_, (&StableId, &Inventory, &mut Modifiers, &mut ModifierClocks)>,
    mut held: Local<'_, Vec<ModifierId>>,
) {
    let (Some(items), Some(book)) = (items, book) else {
        return;
    };
    if items.modifiers().is_empty() {
        return;
    }
    let now = tick.start();
    for (&id, inventory, modifiers, clocks) in &mut units {
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
                    rank: 1,
                    hold: Some(Hold::Passive),
                };
                let source = sources.get(id);
                let param = |place: &ParamPlace| {
                    params.modifier_param(modifier, None, 1, place, source.as_ref())
                };
                carried.apply(book.application(modifier, applier, None, now, *rate, param));
            }
        }
    }
}

#[cfg(test)]
mod tests;
