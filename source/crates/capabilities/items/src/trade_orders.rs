use bevy_ecs::system::{Local, Res};
use campfire_actions::{ProgressIndexes, SlotsMut, UseGroup};
use campfire_players::{OrderedUnits, PlayerResources, PlayerRowsMut, TickCommands};
use campfire_values::ItemId;
use campfire_values::Team;
use campfire_vision::{ByVision, SeenTargets};

use crate::inventory::{CarrierSlots, Inventory};
use crate::item_book::ItemBook;
use crate::items_directive::ItemsDirective;
use crate::shopping::Shopping;

/// The tick's orders that buy, sell or swap items.
#[derive(Debug)]
pub struct TradeOrders;

/// A carrier, as a trade changes it: its team, which sees the shop, its inventory, its use group
/// and its slots.
type Carrier = (
    Option<&'static Team>,
    &'static mut Inventory,
    &'static UseGroup,
    Option<SlotsMut>,
);

impl TradeOrders {
    /// Applies each buy, sale and swap of the tick, in input order, to each of its units by stable
    /// id, so a later one in the tick sees what an earlier one changed: to a unit its player
    /// controls that carries an inventory, dead or not. A buy and a sale name a living shop unit,
    /// which the unit's vision group sees and which must serve the unit; see `Shopping::serving`.
    /// A buy of an item the shop sells pays its price, which the player affords, and needs room
    /// for the item once the components it gives up left; a sale gives back the shop's share of
    /// the stack's cost. A swap swaps two of the unit's slots anywhere. Each slot whose item type
    /// changes holds its new item's action, or none, afresh; a swapped slot keeps its action's
    /// cooldown. An order that fails a check is dropped: a client can send anything. A client
    /// predicts no trade, as its resources and slots come from the server.
    pub(super) fn trade_items(
        (mut orders, book): (TickCommands<'_, '_, ItemsDirective>, Res<'_, ItemBook>),
        (shopping, visible): (Shopping<'_, '_>, SeenTargets<'_, '_, ByVision>),
        mut resources: PlayerRowsMut<'_, '_, PlayerResources>,
        mut units: OrderedUnits<'_, '_, Carrier>,
        mut indexes: ProgressIndexes<'_>,
        (mut given_up, mut before): (Local<'_, Vec<u32>>, Local<'_, Vec<Option<ItemId>>>),
    ) {
        orders.for_each(|order| {
            let directive = order.directive;
            units.for_each_mut(
                &order,
                |unit, entity, (team, mut inventory, &group, slots)| {
                    let sees = |shop| team.is_some_and(|&team| visible.sees(team, shop));
                    let Some(mut held) = resources.of_player_slot_mut(order.player_slot) else {
                        return;
                    };
                    inventory.note(&mut before);
                    match directive {
                        ItemsDirective::Swap { from, to } => {
                            // A swap of a slot with itself, or past the slots, changes nothing.
                            let held_slots = inventory.slots().len();
                            if from == to || usize::from(from.max(to)) >= held_slots {
                                return;
                            }
                            let carrier = slots.map(|held| CarrierSlots {
                                unit: held,
                                dues: indexes.of(unit, entity),
                                group,
                            });
                            inventory.swap_with(from, to, carrier);
                            return;
                        }
                        ItemsDirective::Buy { item, shop } => {
                            let Some(shop) = shopping.serving(shop, unit).filter(|_| sees(shop))
                            else {
                                return;
                            };
                            let Some(purchase) =
                                shopping.purchase(shop, &inventory, item, &mut given_up)
                            else {
                                return;
                            };
                            let resource = shop.resource();
                            if !purchase.room || held.amount(resource) < purchase.price {
                                return;
                            }
                            held.add(resource, -purchase.price)
                                .expect("a price the player affords takes nothing below zero");
                            inventory.complete(&book, item, &given_up);
                        }
                        ItemsDirective::Sell { stack: slot, shop } => {
                            let Some(shop) = shopping.serving(shop, unit).filter(|_| sees(shop))
                            else {
                                return;
                            };
                            let Some(&Some(carried)) = inventory.slots().get(usize::from(slot))
                            else {
                                return;
                            };
                            let cost = book
                                .get(carried.item)
                                .expect("a carried item is in the book")
                                .cost_in(shop.resource());
                            let refund = shop.refund(cost, carried.count.get());
                            let resource = shop.resource();
                            if held.amount(resource).checked_add(refund).is_none() {
                                return;
                            }
                            held.add(resource, refund)
                                .expect("a refund the player's amount holds");
                            inventory.take(slot);
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
                },
            );
        });
    }
}
