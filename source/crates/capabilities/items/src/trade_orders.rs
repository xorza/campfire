use bevy_ecs::query::Has;
use bevy_ecs::system::{Local, Query, Res};
use campfire_actions::{ActionSlots, SlotTimes, UseGroup};
use campfire_players::{PlayerResources, PlayerRowsMut, TickCommands};
use campfire_sim::{EntityIndex, Position};
use campfire_units::{Dead, Owner};
use campfire_values::Team;

use crate::inventory::{CarrierSlots, Inventory};
use crate::item_book::ItemBook;
use crate::item_id::ItemId;
use crate::items_action::ItemsAction;
use crate::shop_book::ShopBook;

/// The tick's orders that buy, sell or swap items.
#[derive(Debug)]
pub struct TradeOrders;

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
        (mut orders, index): (TickCommands<'_, '_, ItemsAction>, Res<'_, EntityIndex>),
        (book, shop, mut resources): (
            Res<'_, ItemBook>,
            Res<'_, ShopBook>,
            PlayerRowsMut<'_, '_, PlayerResources>,
        ),
        mut units: Query<
            '_,
            '_,
            (
                &Owner,
                &Team,
                &Position,
                &mut Inventory,
                &UseGroup,
                Option<(&mut ActionSlots, &mut SlotTimes)>,
                Has<Dead>,
            ),
        >,
        (mut given_up, mut before): (Local<'_, Vec<u32>>, Local<'_, Vec<Option<ItemId>>>),
    ) {
        orders.for_each(|order| {
            let action = order.action;
            for &unit in order.units {
                let Some(Ok((owner, &team, &pos, mut inventory, &group, mut slots, dead))) =
                    index.get(unit).map(|entity| units.get_mut(entity))
                else {
                    continue;
                };
                if !order.controls(*owner) {
                    continue;
                }
                let Some(mut held) = resources.of_mut(Some(owner)) else {
                    continue;
                };
                inventory.note(&mut before);
                let shop = shop
                    .0
                    .as_ref()
                    .filter(|shop| dead || shop.serves(team, pos));
                match (action, shop) {
                    (ItemsAction::Swap { from, to }, _) => {
                        let carrier = slots.as_mut().map(|(slots, times)| CarrierSlots {
                            slots,
                            times,
                            group,
                        });
                        inventory.swap_with(from, to, carrier);
                        continue;
                    }
                    (ItemsAction::Buy { item }, Some(shop)) if shop.sells(item) => {
                        let Some(price) =
                            inventory.purchase(&book, item, shop.resource, &mut given_up)
                        else {
                            continue;
                        };
                        if held.amount(shop.resource) < price {
                            continue;
                        }
                        held.add(shop.resource, -price)
                            .expect("a price the player affords takes nothing below zero");
                        inventory.complete(&book, item, &given_up);
                    }
                    (ItemsAction::Sell { slot }, Some(shop)) => {
                        let Some(carried) = inventory.take(slot) else {
                            continue;
                        };
                        let cost = book
                            .get(carried.item)
                            .expect("a carried item is in the book")
                            .cost_in(shop.resource);
                        let refund = shop.refund(cost, carried.count.get());
                        if held.add(shop.resource, refund).is_none() {
                            inventory.restore(slot, carried);
                        }
                    }
                    _ => continue,
                }
                if let Some((slots, times)) = &mut slots {
                    let carrier = CarrierSlots {
                        slots,
                        times,
                        group,
                    };
                    inventory.follow(&book, carrier, &before);
                }
            }
        });
    }
}
