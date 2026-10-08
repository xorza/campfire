use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Has, QueryItem, QueryState, With, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_common::{Tick, Ticks};
use campfire_script::{ScriptError, ScriptId};
use campfire_sim::{
    EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry, TickInputs, TickRate,
};

use crate::abilities::AbilitiesSet;
use crate::actions::ActionsSet;
use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::ActionSlots;
use crate::actions::action_target::ActionTarget;
use crate::actions::range::Range;
use crate::actions::targets::Targets;
use crate::combat::CombatSet;
use crate::items::ItemsSet;
use crate::items::inventory::Inventory;
use crate::items::item_book::ItemBook;
use crate::items::item_id::ItemId;
use crate::items::shop::Shop;
use crate::navigation::destination::Destination;
use crate::navigation::group_box::GroupBox;
use crate::navigation::on_path::OnPath;
use crate::navigation::party::{Party, PartyKey};
use crate::navigation::path_walker::PathWalker;
use crate::navigation::paths::Paths;
use crate::navigation::progress::Progress;
use crate::navigation::route::Route;
use crate::orders::ai::Ai;
use crate::orders::ai_data::AiData;
use crate::orders::error::AiError;
use crate::orders::learning::Learning;
use crate::orders::next_think::NextThink;
use crate::orders::order::Action;
use crate::orders::resetting::Resetting;
use crate::orders::tick_orders::TickOrders;
use crate::orders::unit_order::{OrderedUnit, UnitOrder};
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::production::ProductionSet;
use crate::production::build_specs::BuildSpecs;
use crate::production::builder::Builder;
use crate::production::gatherer::Gatherer;
use crate::production::rally::Rally;
use crate::production::rally_target::RallyTarget;
use crate::production::site::Site;
use crate::production::train_queue::TrainQueue;
use crate::progression::points::Points;
use crate::scripts::ctx::Ctx;
use crate::scripts::error::CallError;
use crate::scripts::hook::Hook;
use crate::scripts::pool::Pool;
use crate::scripts::script_batch::ScriptBatch;
use crate::units::dead::Dead;

use crate::stats::StatsSet;
use crate::stats::level::Level;
use crate::stats::pools::Pools;
use crate::units::body::Body;
use crate::units::by_type::ByType;
use crate::units::owner::Owner;
use crate::units::spawn_point::SpawnPoint;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::bounds::Bounds;

pub(crate) mod ai;
pub(crate) mod ai_data;
pub(crate) mod error;
pub(crate) mod learning;
pub(crate) mod next_think;
pub(crate) mod order;
pub(crate) mod orders_api;
pub(crate) mod resetting;
pub(crate) mod tick_orders;
pub(crate) mod unit_order;

/// The systems of `orders`, for the mode to order its own against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum OrdersSet {
    /// In `SimSet::Inputs`: the tick's orders become current.
    Orders,
}

/// The `orders` capability: units that take orders from a player, or from the AI script of their
/// type.
#[derive(Debug)]
pub struct Orders;

impl Orders {
    /// Adds orders to a match: in Inputs, the tick's orders are read, orders become current, ranks
    /// are learned, and, with production, trains are cancelled and rally points set, and, with
    /// items, on the server, items trade; in Think, the resets whose units arrived end, then the units due this tick think; in Act,
    /// before combat starts attacks, units walk their paths and chase their targets. It builds on
    /// the core `Units` installs, on combat and on navigation, and installs after production and
    /// items, whose actions it applies only when they are installed. Without the core's scripts,
    /// as on a client, no unit thinks and no item trades.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.init_resource::<PlayerOrders>();
        world.init_resource::<TickOrders>();
        schedule.add_systems((
            (
                read_orders,
                check_player_orders,
                apply_player_orders,
                learn_ranks,
            )
                .chain()
                .in_set(SimSet::Inputs)
                .in_set(OrdersSet::Orders)
                .after(StatsSet::Regenerate)
                .after(CombatSet::Respawn),
            (follow_paths, chase)
                .chain()
                .in_set(SimSet::Act)
                .before(CombatSet::Attack),
        ));
        schedule.configure_sets((
            ActionsSet::HoldAtInputs.after(OrdersSet::Orders),
            ItemsSet::HoldAtInputs.after(OrdersSet::Orders),
            ProductionSet::CheckBuilds.after(OrdersSet::Orders),
            OrdersSet::Orders.after(AbilitiesSet::Toggles),
        ));
        registry.register_component::<NextThink>();
        registry.register_component::<Resetting>();
        let production = world.contains_resource::<BuildSpecs>();
        if production {
            schedule.add_systems(
                apply_production_orders
                    .in_set(SimSet::Inputs)
                    .in_set(OrdersSet::Orders)
                    .after(learn_ranks),
            );
        }
        if !world.contains_non_send::<Ctx>() {
            return;
        }
        world.insert_resource(ByType::<Ai>::default());
        schedule.add_systems((end_dead_resets, think).chain().in_set(SimSet::Think));
        if world.contains_resource::<ItemBook>() {
            let trade = trade_items
                .in_set(SimSet::Inputs)
                .in_set(OrdersSet::Orders)
                .after(learn_ranks);
            if production {
                schedule.add_systems(trade.after(apply_production_orders));
            } else {
                schedule.add_systems(trade);
            }
        }
    }

    /// The think period of `data` at `rate`, a tick at the least, for a script that defines
    /// `on_think` when `thinks`: what an AI loads with.
    pub(crate) fn ai_period(data: &AiData, rate: TickRate, thinks: bool) -> Result<Ticks, AiError> {
        let period = rate.duration(data.think_ms).ok_or(AiError::TimeTooLarge)?;
        if !thinks {
            return Err(AiError::NoThink);
        }
        Ok(period)
    }

    /// Applies `order`, which its source checked, to the unit of `entity` in `now`, as every
    /// order applies; a unit that resets takes none.
    pub(crate) fn apply_order(world: &mut World, entity: Entity, order: UnitOrder, now: Tick) {
        let bounds = *world.resource::<Bounds>();
        let mut unit = world.entity_mut(entity);
        if unit.contains::<Resetting>() {
            return;
        }
        let parts = unit
            .get_components_mut::<Ordered>()
            .expect("an ordered unit stands");
        if order.apply(Orders::ordered(parts), &bounds, now) {
            unit.insert(Resetting);
        }
    }

    /// The unit an order reads and changes, of its `parts`.
    fn ordered<'a>(
        (&at, spawn, slots, walker, destination, route, progress, builder, gatherer): QueryItem<
            'a,
            '_,
            Ordered,
        >,
    ) -> OrderedUnit<'a> {
        OrderedUnit {
            at,
            spawn: spawn.map(|spawn| spawn.get()),
            slots,
            walker,
            destination,
            route,
            progress,
            builder,
            gatherer,
        }
    }
}

/// The orders the tick's inputs give, in input order, each checked as a player's order needs, for
/// `apply_player_orders` to apply. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
struct PlayerOrders(Vec<(Entity, UnitOrder)>);

/// Reads the orders of the tick's inputs, once, for each system of orders that applies them.
fn read_orders(inputs: Res<'_, TickInputs>, mut orders: ResMut<'_, TickOrders>) {
    orders.read(&inputs);
}

/// The parts of a unit a player's order checks.
type Commanded = (
    &'static Owner,
    Option<&'static Team>,
    &'static Position,
    Has<Destination>,
    Option<&'static ActionSlots>,
);

/// A unit of a group's move: its entity and where it stands.
#[derive(Debug, Clone, Copy)]
struct Mover {
    entity: Entity,
    at: Position,
}

/// Checks each order of the tick, in input order, so a later order in the tick wins, for each of
/// its units, by stable id. An order to a unit its player does not control, that is dead or
/// resets, is dropped, and so are a move of a unit with nowhere to walk, an attack on a unit that
/// is not a living enemy or that none of its weapons selects, a slot's action of a kind other
/// than a cast, a train or a gather, a gather at no unit, and a build of a slot that holds none:
/// a client can send anything. A move to two units or more that walk moves them as a group: each
/// walks to its own goal by the group's box, as one party, the order's.
fn check_player_orders(
    (tick, bounds, orders, index): (
        Res<'_, SimTick>,
        Res<'_, Bounds>,
        Res<'_, TickOrders>,
        Res<'_, EntityIndex>,
    ),
    book: Res<'_, ActionBook>,
    targets: Targets<'_, '_>,
    units: Query<'_, '_, Commanded, (Without<Dead>, Without<Resetting>)>,
    mut checked: ResMut<'_, PlayerOrders>,
    mut movers: Local<'_, Vec<Mover>>,
) {
    let now = tick.start();
    for order in orders.iter() {
        if !order.action.to_units() {
            continue;
        }
        let controlled = order
            .units
            .iter()
            .filter_map(|&id| {
                let entity = index.get(id)?;
                Some((entity, units.get(entity).ok()?))
            })
            .filter(|(_, (owner, ..))| owner.slot() == order.slot);
        if let Action::Move { x, z } = order.action {
            movers.clear();
            movers.extend(
                controlled
                    .filter(|(_, (.., walks, _))| *walks)
                    .map(|(entity, (_, _, &at, ..))| Mover { entity, at }),
            );
            let Some(group) = GroupBox::of(movers.iter().map(|mover| mover.at)) else {
                continue;
            };
            if let &[Mover { entity, .. }] = movers.as_slice() {
                checked
                    .0
                    .push((entity, UnitOrder::Move { x, z, party: None }));
                continue;
            }
            let goal = bounds.clamp_ground([x, z]);
            let key = PartyKey::Order {
                tick: now,
                slot: order.slot,
                number: order.number,
            };
            for &Mover { entity, at } in &*movers {
                let [x, z] = group.goal_of(at, goal, *bounds);
                let party = Party {
                    key,
                    goal: bounds.ground_point(goal, at),
                };
                checked.0.push((
                    entity,
                    UnitOrder::Move {
                        x,
                        z,
                        party: Some(party),
                    },
                ));
            }
            continue;
        }
        for (entity, (_, team, _, _, slots)) in controlled {
            let unit_order = match order.action {
                Action::Attack { target } => {
                    let selected = team.and_then(|&team| {
                        let unit = targets.enemy(team, target)?;
                        Some((targets.attitude(team, unit.team), unit.tags))
                    });
                    let armed = slots.is_some_and(|slots| {
                        selected.is_some() && book.weapon_for(slots, selected).is_some()
                    });
                    armed.then_some(UnitOrder::Attack { target })
                }
                Action::Slot { slot, target } => {
                    let kind = slots
                        .and_then(|slots| slots.slot(slot))
                        .and_then(|held| book.get(held.action?))
                        .map(|action| action.kind.kind());
                    match (kind, target) {
                        (Some(ActionKind::Cast | ActionKind::Train), _) => {
                            Some(UnitOrder::Slot { slot, target })
                        }
                        (Some(ActionKind::Gather), ActionTarget::Unit(target)) => {
                            Some(UnitOrder::Gather { slot, target })
                        }
                        _ => None,
                    }
                }
                Action::Build { slot, target } => {
                    let kind = slots
                        .and_then(|slots| slots.slot(slot))
                        .and_then(|held| book.get(held.action?))
                        .map(|action| action.kind.kind());
                    (kind == Some(ActionKind::Build)).then_some(UnitOrder::Build { slot, target })
                }
                Action::Stop => Some(UnitOrder::Stop),
                Action::Move { .. }
                | Action::Learn { .. }
                | Action::Buy { .. }
                | Action::Sell { .. }
                | Action::Swap { .. }
                | Action::CancelTrain { .. }
                | Action::Rally { .. }
                | Action::CancelBuild => unreachable!("a move and the others' actions left"),
            };
            checked.0.extend(unit_order.map(|order| (entity, order)));
        }
    }
}

/// The parts of a unit that an order reads and changes.
type Ordered = (
    &'static Position,
    Option<&'static SpawnPoint>,
    Option<&'static mut ActionSlots>,
    Option<&'static mut PathWalker>,
    Option<&'static mut Destination>,
    Option<&'static mut Route>,
    Option<&'static mut Progress>,
    Option<&'static mut Builder>,
    Option<&'static mut Gatherer>,
);

/// Applies the tick's checked player orders, in input order, each as every order applies; a
/// player orders no reset.
fn apply_player_orders(
    tick: Res<'_, SimTick>,
    bounds: Res<'_, Bounds>,
    mut checked: ResMut<'_, PlayerOrders>,
    mut units: Query<'_, '_, Ordered>,
) {
    let now = tick.start();
    for (entity, order) in checked.0.drain(..) {
        let parts = units.get_mut(entity).expect("a checked order's unit");
        let resets = order.apply(Orders::ordered(parts), &bounds, now);
        debug_assert!(!resets, "a player orders no reset");
    }
}

/// Applies each learn order of the tick, in input order, to each of its units by stable id, so a
/// second learn in a tick sees the point the first spent: to a unit its player controls, dead or
/// not, the next rank of the action in the slot, for a point, when the unit's level reaches the
/// one the slot's kind gives that rank. It changes nothing under way. An order that fails a check
/// is dropped: a client can send anything.
fn learn_ranks(
    orders: Res<'_, TickOrders>,
    index: Res<'_, EntityIndex>,
    learning: Learning<'_>,
    mut units: Query<'_, '_, (&Owner, &mut ActionSlots, &mut Points, &Level)>,
) {
    for order in orders.iter() {
        let Action::Learn { slot } = order.action else {
            continue;
        };
        for &unit in order.units {
            let Some(Ok((owner, mut slots, mut points, &level))) =
                index.get(unit).map(|entity| units.get_mut(entity))
            else {
                continue;
            };
            let learnable = slots
                .slot(slot)
                .is_some_and(|held| learning.learnable(held, *points, level));
            if owner.slot() != order.slot || !learnable {
                continue;
            }
            points.spend();
            slots.learn(slot);
        }
    }
}

/// Applies each production order of the tick, in input order, to each of its units by stable id
/// that its player controls: a cancel of a train or a rally to a unit with a train queue, dead or
/// not, a cancel of a build to a living site, as a dead one refunds nothing. A cancel of a train
/// names an entry by its place in the queue as it stands; a place past its end is ignored. Its
/// entry leaves the queue and its player gets back the player resources it paid; a head's leaving
/// starts the next one's time in this tick. A cancel of a build gives back the build's
/// `cancel_refund` of each player resource its build paid, each rounded down, and despawns the
/// site, with no death. A refund that would carry an amount past an `i64` refuses its cancel. A
/// rally sets the producer's rally point, a point taken into the bounds, or a unit, or clears it.
fn apply_production_orders(
    (tick, bounds, orders, index): (
        Res<'_, SimTick>,
        Res<'_, Bounds>,
        Res<'_, TickOrders>,
        Res<'_, EntityIndex>,
    ),
    builds: Res<'_, BuildSpecs>,
    mut resources: Option<ResMut<'_, PlayerResources>>,
    mut units: Query<'_, '_, (&Owner, Option<&mut TrainQueue>, Option<&Site>, Has<Dead>)>,
    mut commands: Commands<'_, '_>,
    (mut refund, mut cancelled): (Local<'_, Vec<ResourceAmount>>, Local<'_, Vec<Entity>>),
) {
    let now = tick.start();
    cancelled.clear();
    let mut refunds = |amounts: &[ResourceAmount], slot| {
        amounts.is_empty()
            || resources
                .as_deref_mut()
                .expect("an order that paid resources runs in a match with them")
                .refund(slot, amounts)
    };
    for order in orders.iter() {
        if !matches!(
            order.action,
            Action::CancelTrain { .. } | Action::Rally { .. } | Action::CancelBuild
        ) {
            continue;
        }
        for &unit in order.units {
            let Some(entity) = index.get(unit) else {
                continue;
            };
            let Ok((owner, queue, site, dead)) = units.get_mut(entity) else {
                continue;
            };
            if owner.slot() != order.slot {
                continue;
            }
            match (order.action, queue, site) {
                (Action::CancelTrain { place }, Some(mut queue), _) => {
                    let place = usize::from(place);
                    let Some(paid) = queue.paid(place) else {
                        continue;
                    };
                    if refunds(paid, order.slot) {
                        queue.remove(place, now);
                    }
                }
                (Action::Rally { target }, Some(_), _) => {
                    let target = target.map(|target| match target {
                        RallyTarget::Point { x, z } => {
                            let [x, z] = bounds.clamp_ground([x, z]);
                            RallyTarget::Point { x, z }
                        }
                        unit @ RallyTarget::Unit(_) => unit,
                    });
                    match target {
                        Some(target) => commands.entity(entity).insert(Rally::new(target)),
                        None => commands.entity(entity).remove::<Rally>(),
                    };
                }
                (Action::CancelBuild, _, Some(site)) if !dead && !cancelled.contains(&entity) => {
                    let spec = builds
                        .of(site.action())
                        .expect("a site's build is in the book");
                    refund.clear();
                    refund.extend(site.paid().iter().map(|paid| ResourceAmount {
                        resource: paid.resource,
                        amount: spec.refund.of(paid.amount),
                    }));
                    if refunds(&refund, order.slot) {
                        commands.entity(entity).despawn();
                        cancelled.push(entity);
                    }
                }
                _ => {}
            }
        }
    }
}

/// Applies each buy, sale and swap of the tick, in input order, to each of its units by stable id,
/// so a later one in the tick sees what an earlier one changed: to a unit its player controls that
/// carries an inventory, dead or not. A buy of an item the shop sells, and a sale, need the unit
/// dead or in a shop of its team; a buy pays its price, which the player affords, and needs room
/// for the item once the components it gives up left; a sale gives back the shop's share of the
/// stack's cost. A swap swaps two of the unit's slots anywhere. Each slot whose item type changes
/// holds its new item's action, or none, afresh; a swapped slot keeps its action's cooldown. An
/// order that fails a check is dropped: a client can send anything. A client predicts no trade, as
/// its resources and slots come from the server.
fn trade_items(
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
                    let Some(price) = inventory.purchase(&book, item, shop.resource, &mut given_up)
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

/// A unit due to think this tick: since which tick, and with which script and period.
#[derive(Debug, Clone, Copy)]
struct Due {
    since: Tick,
    id: StableId,
    entity: Entity,
    script: ScriptId,
    period: Ticks,
}

/// Ends the reset of each unit that died since the last Think stage, with nothing more: its AI
/// thinks free of it when it lives again. No order reaches a dead unit, so it starts no reset.
fn end_dead_resets(
    mut commands: Commands<'_, '_>,
    units: Query<'_, '_, Entity, (Added<Dead>, With<Resetting>)>,
) {
    for entity in &units {
        commands.entity(entity).remove::<Resetting>();
    }
}

/// Runs `on_think` for each living unit of a type with AI that is due, those due longest first,
/// then by stable id. A unit is first due in the first tick that leaves the remainder of its
/// stable id when divided by its type's period, so the units of a type spread over the period;
/// then a period after each think. Each call's orders apply when it returns; a failed call's do
/// not. A unit whose call finds the think pool spent stays due, so under load AI thinks later,
/// and no unit misses its turn for good. First, each reset whose unit arrived, its destination
/// dropped, ends with its pools full.
fn think(
    world: &mut World,
    resetting: &mut QueryState<(Entity, Option<&Destination>), (With<Resetting>, Without<Dead>)>,
    thinkers: &mut QueryState<(Entity, &StableId, &UnitType, Option<&NextThink>), Without<Dead>>,
    (mut due, mut reset): (Local<'_, Vec<Due>>, Local<'_, Vec<Entity>>),
) {
    let now = world.resource::<SimTick>().start();
    reset.clear();
    reset.extend(
        resetting
            .iter(world)
            .filter(|(_, destination)| {
                destination.is_none_or(|destination| destination.get().is_none())
            })
            .map(|(entity, _)| entity),
    );
    due.clear();
    let book = world.resource::<ByType<Ai>>();
    for (entity, &id, &unit_type, next) in thinkers.iter(world) {
        let Some(ai) = book.get(unit_type) else {
            continue;
        };
        let period = ai.period;
        let since = match next {
            Some(next) => next.get(),
            None if now.get() % period.get() == id.get() % period.get() => now,
            None => continue,
        };
        if since <= now {
            due.push(Due {
                since,
                id,
                entity,
                script: ai.script,
                period,
            });
        }
    }
    for &entity in &*reset {
        let mut unit = world.entity_mut(entity);
        unit.remove::<Resetting>();
        if let Some(mut pools) = unit.get_mut::<Pools>() {
            pools.fill();
        }
    }
    if due.is_empty() {
        return;
    }
    due.sort_unstable_by_key(|due| (due.since, due.id));
    let ctx = world.non_send::<Ctx>().clone();
    ScriptBatch::run(world, ctx.view(), |batch| {
        for &Due {
            since,
            id,
            entity,
            script,
            period,
        } in &*due
        {
            // A unit with no position or team is no unit scripts see.
            let Some(unit) = ctx.view().unit(id) else {
                continue;
            };
            ctx.frame().begin_think(batch.world(), id);
            let next = match batch.call(Pool::Think, script, Hook::OnThink, (ctx.clone(), unit)) {
                Ok(_) => {
                    ctx.apply(batch.world(), now);
                    now.after(period)
                }
                Err(ScriptError::TickBudget) => since,
                Err(error) => {
                    batch.record(Some(id), Hook::OnThink, CallError::from_script(error));
                    now.after(period)
                }
            };
            let world = batch.world();
            match world.get_mut::<NextThink>(entity) {
                Some(mut due) => *due = NextThink::new(next),
                None => {
                    world.entity_mut(entity).insert(NextThink::new(next));
                }
            }
        }
    });
}

/// Sends each path walker with no attack target and no cast walking in range, on its path, to the
/// waypoint it walks to, and on to the next once the waypoint is within its body, or it stands on
/// the waypoint with no body: walkers that push each other never stand on one point. A walker that
/// chased a target walks back to where it left its path.
fn follow_paths(
    paths: Res<'_, Paths>,
    mut walkers: Query<
        '_,
        '_,
        (
            &Position,
            &OnPath,
            &mut PathWalker,
            Option<&ActionSlots>,
            &mut Destination,
            Option<&Route>,
            Option<&Body>,
        ),
        Without<Dead>,
    >,
) {
    for (&position, path, mut walker, slots, mut destination, route, body) in &mut walkers {
        let busy = |slots: &ActionSlots| slots.attack_target().is_some() || slots.approaching();
        if walker.left() || slots.is_some_and(busy) {
            continue;
        }
        let path = path.get();
        let mut waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
        if waypoint.is_some_and(|at| position.within_ground(at, Body::shape_of(body).bound())) {
            walker.advance();
            waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
        }
        Destination::walk_to(&mut destination, route, waypoint);
    }
}

/// Walks each unit that can move to its attack target while out of the range of the weapon it
/// attacks it with, and stops it in range or in its windup; a cast that walks in range walks the
/// unit instead. A unit whose target is gone, dead, no longer an enemy or one no weapon of it
/// selects drops it and stops.
fn chase(
    book: Res<'_, ActionBook>,
    targets: Targets<'_, '_>,
    mut chasers: Query<
        '_,
        '_,
        (
            &Position,
            &Team,
            &mut ActionSlots,
            &mut Destination,
            Option<&Route>,
            Option<&Body>,
        ),
        Without<Dead>,
    >,
) {
    for (&position, &team, mut slots, mut destination, route, body) in &mut chasers {
        let Some(target) = slots.attack_target() else {
            continue;
        };
        if slots.attacking().is_some() || slots.approaching() {
            continue;
        }
        let aimed = targets.enemy(team, target).and_then(|unit| {
            let selected = (targets.attitude(team, unit.team), unit.tags);
            let slot = book.weapon_for(&slots, Some(selected))?;
            Some((unit, book.range(&slots, slot)))
        });
        match aimed {
            None => {
                slots.set_attack_target(None);
                Destination::walk_to(&mut destination, route, None);
            }
            Some((unit, Range::Meters(range)))
                if !targets.reaches(position, Body::shape_of(body), range, &unit) =>
            {
                Destination::walk_to(&mut destination, route, Some(unit.pos));
            }
            Some(_) => Destination::walk_to(&mut destination, route, None),
        }
    }
}

#[cfg(any(test, feature = "internals"))]
pub(crate) mod internals {
    use crate::orders::Orders;
    use crate::orders::ai::Ai;
    use crate::orders::ai_data::AiData;
    use crate::orders::error::AiError;
    use crate::scripts::script_book::ScriptBook;
    use crate::units::by_type::ByType;
    use crate::units::unit_type::UnitType;
    use bevy_ecs::world::World;
    use campfire_script::ScriptId;
    use campfire_sim::TickRate;

    impl Orders {
        /// Gives `unit_type` its AI, with its compiled script: the think period in milliseconds
        /// becomes whole ticks at the match's rate, rounded up, and at least one.
        pub fn load_ai(
            world: &mut World,
            unit_type: UnitType,
            data: &AiData,
            script: ScriptId,
        ) -> Result<(), AiError> {
            let rate = *world.resource::<TickRate>();
            let ai = Ai::of(data, script, world.resource::<ScriptBook>(), rate)?;
            world.resource_mut::<ByType<Ai>>().set(unit_type, ai);
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests;
