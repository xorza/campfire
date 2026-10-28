use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, Without};
use bevy_ecs::system::{Commands, Local, Query, Res};
use campfire_declarations::ById;
use campfire_players::{
    OrderedUnits, PlayerResources, PlayerRowsMut, ResourceAmount, TickCommands,
};
use campfire_sim::{SimTick, StableId};
use campfire_units::{Dead, Owner, RiderFate, RidersCalledOut, StatusTags};
use campfire_values::UnitType;

use crate::Production;
use crate::production_directive::ProductionDirective;
use crate::sell_data::SellSpec;
use crate::site::Site;
use crate::sold::Sold;
use crate::train_queue::{QueueIndex, TrainQueue};

/// The sales of buildings (04-capabilities/production.md, A building may be sold).
#[derive(Debug)]
pub(crate) struct Sales;

/// What a sale reads of a unit its player orders: its type, its train queue, whether it is a
/// site, sold or dead, and its status tags.
type SoldUnits<'w, 's> = OrderedUnits<
    'w,
    's,
    (
        &'static UnitType,
        Option<&'static mut TrainQueue>,
        (Has<Site>, Has<Sold>, Has<Dead>),
        Option<&'static mut StatusTags>,
    ),
>;

impl Sales {
    /// Starts each sale of the tick, in input order, of each of its units by stable id that its
    /// player controls, of a type of a sale, living, no site and not sold: its train queue
    /// refunded at once, each entry its player gets back what it paid, from its head, as
    /// `BuildAssistant::sellObject` refunds its production, an entry whose refund would carry an
    /// amount past an `i64` staying with those behind it; its riders are called out; it is marked
    /// sold until its sale's time
    /// passes, and its engine tag `constructing` blocks its attacks, casts, uses and trains. A sale
    /// is the server's.
    pub(super) fn start_sales(
        tick: Res<'_, SimTick>,
        (mut orders, sales): (
            TickCommands<'_, '_, ProductionDirective>,
            Res<'_, ById<UnitType, SellSpec>>,
        ),
        mut resources: PlayerRowsMut<'_, '_, PlayerResources>,
        mut units: SoldUnits<'_, '_>,
        mut commands: Commands<'_, '_>,
        (mut paid, mut started): (Local<'_, Vec<ResourceAmount>>, Local<'_, Vec<Entity>>),
        mut queues: QueueIndex<'_>,
    ) {
        let now = tick.start();
        started.clear();
        orders.for_each(|order| {
            if order.directive != ProductionDirective::Sell {
                return;
            }
            units.for_each_mut(&order, |unit, entity, (&unit_type, queue, held, status)| {
                let (site, sold, dead) = held;
                let Some(spec) = sales.get(unit_type) else {
                    return;
                };
                if site || sold || dead || started.contains(&entity) {
                    return;
                }
                if let Some(mut queue) = queue {
                    while let Some(entry) = queue.paid(0) {
                        if !entry.is_empty() {
                            paid.clear();
                            paid.extend_from_slice(entry);
                            let player = resources.of_player_slot_mut(order.player_slot);
                            let mut player = player
                                .expect("an entry that paid resources runs in a match with them");
                            if !player.refund(&paid) {
                                break;
                            }
                        }
                        queue.remove(0, now, &mut queues.of(unit, entity));
                    }
                }
                // A sold container's riders leave (exits-docks-and-selection.md, A sold container
                // lets its riders out).
                commands.trigger(RidersCalledOut {
                    entity,
                    fate: RiderFate::Leave,
                });
                let sold = Sold::new(now, spec.time);
                if let Some(mut status) = status {
                    let turned = status.turned(Production::CONSTRUCTING, true);
                    status.set_if_neq(turned);
                    commands.entity(entity).insert(sold);
                } else {
                    let status = StatusTags::from([Production::CONSTRUCTING]);
                    commands.entity(entity).insert((sold, status));
                }
                started.push(entity);
            });
        });
    }

    /// Ends each sale whose time passed, by stable id: its player gets its type's refund, and it
    /// goes with no death, as `BuildAssistant::update` destroys a sold object, the riders still
    /// in it with it; a refund that would
    /// carry an amount past an `i64` is not paid. A sold unit that died first refunds nothing.
    pub(super) fn end_sales(
        tick: Res<'_, SimTick>,
        sales: Res<'_, ById<UnitType, SellSpec>>,
        sold: Query<'_, '_, (Entity, &StableId, &Sold, &UnitType, &Owner), Without<Dead>>,
        mut resources: PlayerRowsMut<'_, '_, PlayerResources>,
        mut commands: Commands<'_, '_>,
        mut ended: Local<'_, Vec<(StableId, Entity)>>,
    ) {
        let now = tick.start();
        ended.clear();
        ended.extend(
            (sold.iter())
                .filter(|(_, _, sold, _, _)| sold.until() <= now)
                .map(|(entity, &id, ..)| (id, entity)),
        );
        ended.sort_unstable();
        for &(_, entity) in &*ended {
            let (_, _, _, &unit_type, &owner) = sold.get(entity).expect("an ended sale's unit");
            let spec = sales
                .get(unit_type)
                .expect("a sold unit's type has its sale");
            if let Some(mut player) = resources.of_player_slot_mut(owner.player_slot()) {
                player.refund(&spec.refund);
            }
            commands.trigger(RidersCalledOut {
                entity,
                fate: RiderFate::Go,
            });
            commands.entity(entity).despawn();
        }
    }
}
