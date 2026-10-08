use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::{EntityIndex, Position};

use crate::actions::action_slots::ActionSlots;
use crate::items::inventory::Inventory;
use crate::items::item_book::ItemBook;
use crate::items::item_id::ItemId;
use crate::items::shop::Shop;
use crate::orders::order::Action;
use crate::orders::tick_orders::TickOrders;
use crate::players::player_resources::PlayerResources;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::team::Team;

/// The tick's orders that buy, sell or swap items.
#[derive(Debug)]
pub(super) struct TradeOrders;

impl TradeOrders {
    /// Applies each buy, sale and swap of the tick, in input order, to each of its units by stable
    /// id, so a later one in the tick sees what an earlier one changed: to a unit its player
    /// controls that carries an inventory, dead or not. A buy of an item the shop sells, and a
    /// sale, need the unit dead or in a shop of its team; a buy pays its price, which the player
    /// affords, and needs room for the item once the components it gives up left; a sale gives back
    /// the shop's share of the stack's cost. A swap swaps two of the unit's slots anywhere. Each
    /// slot whose item type changes holds its new item's action, or none, afresh; a swapped slot
    /// keeps its action's cooldown. An order that fails a check is dropped: a client can send
    /// anything. A client predicts no trade, as its resources and slots come from the server.
    pub(super) fn trade_items(
        (orders, index): (Res<'_, TickOrders>, Res<'_, EntityIndex>),
        (book, shop, resources): (
            Res<'_, ItemBook>,
            Option<Res<'_, Shop>>,
            Option<ResMut<'_, PlayerResources>>,
        ),
        mut units: Query<
            '_,
            '_,
            (
                &Owner,
                &Team,
                &Position,
                &mut Inventory,
                Option<&mut ActionSlots>,
                Has<Dead>,
            ),
        >,
        (mut given_up, mut before): (Local<'_, Vec<u32>>, Local<'_, Vec<Option<ItemId>>>),
    ) {
        let Some(mut resources) = resources else {
            return;
        };
        for order in orders.iter() {
            let action = order.action;
            if !matches!(
                action,
                Action::Buy { .. } | Action::Sell { .. } | Action::Swap { .. }
            ) {
                continue;
            }
            for &unit in order.units {
                let Some(Ok((owner, &team, &pos, mut inventory, mut slots, dead))) =
                    index.get(unit).map(|entity| units.get_mut(entity))
                else {
                    continue;
                };
                if owner.slot() != order.slot {
                    continue;
                }
                inventory.note(&mut before);
                let shop = shop
                    .as_deref()
                    .filter(|shop| dead || shop.serves(team, pos));
                match (action, shop) {
                    (Action::Swap { from, to }, _) => {
                        inventory.swap_with(from, to, slots.as_deref_mut());
                        continue;
                    }
                    (Action::Buy { item }, Some(shop)) if shop.sells(item) => {
                        let Some(price) =
                            inventory.purchase(&book, item, shop.resource, &mut given_up)
                        else {
                            continue;
                        };
                        if resources.amount(order.slot, shop.resource) < price {
                            continue;
                        }
                        resources
                            .add(order.slot, shop.resource, -price)
                            .expect("a price the player affords takes nothing below zero");
                        inventory.complete(&book, item, &given_up);
                    }
                    (Action::Sell { slot }, Some(shop)) => {
                        let Some(carried) = inventory.take(slot) else {
                            continue;
                        };
                        let cost = book
                            .get(carried.item)
                            .expect("a carried item is in the book")
                            .cost_in(shop.resource);
                        let refund = shop.refund(cost, carried.count.get());
                        if resources.add(order.slot, shop.resource, refund).is_none() {
                            inventory.restore(slot, carried);
                        }
                    }
                    _ => continue,
                }
                if let Some(slots) = slots.as_deref_mut() {
                    inventory.follow(&book, slots, &before);
                }
            }
        }
    }
}
