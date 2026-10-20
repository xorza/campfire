use bevy_ecs::system::{Local, Query, Res};
use campfire_actions::{ProgressIndexes, SlotsMut, UseGroup};
use campfire_players::{PlayerResources, PlayerRowsMut, TickCommands};
use campfire_sim::EntityIndex;
use campfire_units::Owner;

use crate::inventory::{CarrierSlots, Inventory};
use crate::item_book::ItemBook;
use crate::items_directive::ItemsDirective;
use crate::shopping::Shopping;
use campfire_values::ItemId;

/// The tick's orders that buy, sell or swap items.
#[derive(Debug)]
pub struct TradeOrders;

/// A carrier, as a trade changes it.
type Carriers<'w, 's> = Query<
    'w,
    's,
    (
        &'static Owner,
        &'static mut Inventory,
        &'static UseGroup,
        Option<SlotsMut>,
    ),
>;

impl TradeOrders {
    /// Applies each buy, sale and swap of the tick, in input order, to each of its units by stable
    /// id, so a later one in the tick sees what an earlier one changed: to a unit its player
    /// controls that carries an inventory, dead or not. A buy and a sale name a living shop unit,
    /// which must serve the unit; see `Shopping::serving`. A buy of an item the shop sells pays
    /// its price, which the player affords, and needs room for the item once the components it
    /// gives up left; a sale gives back the shop's share of the stack's cost. A swap swaps two of
    /// the unit's slots anywhere. Each slot whose item type changes holds its new item's action,
    /// or none, afresh; a swapped slot keeps its action's cooldown. An order that fails a check
    /// is dropped: a client can send anything. A client predicts no trade, as its resources and
    /// slots come from the server.
    pub(super) fn trade_items(
        (mut orders, index, book): (
            TickCommands<'_, '_, ItemsDirective>,
            Res<'_, EntityIndex>,
            Res<'_, ItemBook>,
        ),
        shopping: Shopping<'_, '_>,
        mut resources: PlayerRowsMut<'_, '_, PlayerResources>,
        mut units: Carriers<'_, '_>,
        mut indexes: ProgressIndexes<'_>,
        (mut given_up, mut before): (Local<'_, Vec<u32>>, Local<'_, Vec<Option<ItemId>>>),
    ) {
        orders.for_each(|order| {
            let directive = order.directive;
            for &unit in order.units {
                let Some(entity) = index.get(unit) else {
                    continue;
                };
                let Ok((owner, mut inventory, &group, slots)) = units.get_mut(entity) else {
                    continue;
                };
                if !order.controls(*owner) {
                    continue;
                }
                let Some(mut held) = resources.of_mut(Some(owner)) else {
                    continue;
                };
                inventory.note(&mut before);
                match directive {
                    ItemsDirective::Swap { from, to } => {
                        let carrier = slots.map(|held| CarrierSlots {
                            unit: held,
                            dues: indexes.of(unit, entity),
                            group,
                        });
                        inventory.swap_with(from, to, carrier);
                        continue;
                    }
                    ItemsDirective::Buy { item, shop } => {
                        let Some(shop) = shopping.serving(shop, unit) else {
                            continue;
                        };
                        let Some(purchase) =
                            shopping.purchase(shop, &inventory, item, &mut given_up)
                        else {
                            continue;
                        };
                        let resource = shop.resource();
                        if !purchase.room || held.amount(resource) < purchase.price {
                            continue;
                        }
                        held.add(resource, -purchase.price)
                            .expect("a price the player affords takes nothing below zero");
                        inventory.complete(&book, item, &given_up);
                    }
                    ItemsDirective::Sell { slot, shop } => {
                        let Some(shop) = shopping.serving(shop, unit) else {
                            continue;
                        };
                        let Some(carried) = inventory.take(slot) else {
                            continue;
                        };
                        let cost = book
                            .get(carried.item)
                            .expect("a carried item is in the book")
                            .cost_in(shop.resource());
                        let refund = shop.refund(cost, carried.count.get());
                        if held.add(shop.resource(), refund).is_none() {
                            inventory.restore(slot, carried);
                        }
                    }
                }
                if let Some(held) = slots {
                    let carrier = CarrierSlots {
                        unit: held,
                        dues: indexes.of(unit, entity),
                        group,
                    };
                    inventory.follow(&book, carrier, &before);
                }
            }
        });
    }
}
