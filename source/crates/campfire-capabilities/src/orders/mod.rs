use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Has, QueryState, With, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use bevy_ecs::world::{Mut, World};
use campfire_common::{Tick, Ticks};
use campfire_script::{ScriptError, ScriptId};
use campfire_sim::{
    EntityIndex, Position, SimSet, SimTick, StableId, StateRegistry, TickInputs, TickRate,
};

use crate::actions::ActionsSet;
use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::ActionSlots;
use crate::actions::range::Range;
use crate::actions::slot_kinds::SlotKinds;
use crate::actions::targets::Targets;
use crate::combat::CombatSet;
use crate::navigation::destination::Destination;
use crate::navigation::on_path::OnPath;
use crate::navigation::path_walker::PathWalker;
use crate::navigation::paths::Paths;
use crate::navigation::route::Route;
use crate::orders::ai::Ai;
use crate::orders::ai_data::AiData;
use crate::orders::error::AiError;
use crate::orders::next_think::NextThink;
use crate::orders::order::{Action, Order};
use crate::orders::resetting::Resetting;
use crate::orders::unit_order::{OrderedUnit, UnitOrder};
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
pub(crate) mod next_think;
pub(crate) mod order;
pub(crate) mod orders_api;
pub(crate) mod resetting;
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
    /// Adds orders to a match: in Inputs, orders become current and ranks are learned; in Think,
    /// the resets whose units arrived end, then the units due this tick think; in Act, before
    /// combat starts attacks, units walk their paths and chase their targets. It builds on the core `Units` installs, on
    /// combat and on navigation. Without the core's scripts, as on a client, no unit thinks.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.init_resource::<PlayerOrders>();
        schedule.add_systems((
            (check_player_orders, apply_player_orders, learn_ranks)
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
        schedule.configure_sets(ActionsSet::HoldAtInputs.after(OrdersSet::Orders));
        registry.register_component::<NextThink>();
        registry.register_component::<Resetting>();
        if !world.contains_non_send::<Ctx>() {
            return;
        }
        world.insert_resource(ByType::<Ai>::default());
        schedule.add_systems((end_dead_resets, think).chain().in_set(SimSet::Think));
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

    /// Applies `order`, which its source checked, to the unit of `entity`, as every order
    /// applies; a unit that resets takes none.
    fn apply_order(world: &mut World, entity: Entity, order: UnitOrder) {
        let bounds = *world.resource::<Bounds>();
        let mut unit = world.entity_mut(entity);
        if unit.contains::<Resetting>() {
            return;
        }
        let (&at, spawn, slots, walker, destination) = unit
            .get_components_mut::<Ordered>()
            .expect("an ordered unit stands");
        let ordered = OrderedUnit {
            at,
            spawn: spawn.map(|spawn| spawn.get()),
            slots,
            walker,
            destination,
        };
        if order.apply(ordered, &bounds) {
            unit.insert(Resetting);
        }
    }
}

/// The orders the tick's inputs give, in input order, each checked as a player's order needs, for
/// `apply_player_orders` to apply. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
struct PlayerOrders(Vec<(Entity, UnitOrder)>);

/// Checks each order the tick's inputs give, in input order, so a later order in the tick wins.
/// An order to a unit its player does not control, that is dead or resets, is dropped, and so are
/// a body that is not an order, a move of a unit with nowhere to walk, an attack on a unit that is
/// not a living enemy or that none of its weapons selects, and a slot's action of a kind other
/// than a cast or a train: a client can send anything.
fn check_player_orders(
    inputs: Res<'_, TickInputs>,
    index: Res<'_, EntityIndex>,
    book: Res<'_, ActionBook>,
    targets: Targets<'_, '_>,
    units: Query<
        '_,
        '_,
        (
            &Owner,
            Option<&Team>,
            Has<Destination>,
            Option<&ActionSlots>,
        ),
        (Without<Dead>, Without<Resetting>),
    >,
    mut checked: ResMut<'_, PlayerOrders>,
) {
    for command in inputs.commands(Order::CAPABILITY) {
        let Some(order) = Order::decode(command.body) else {
            continue;
        };
        let Some(Ok((owner, team, walks, slots))) =
            index.get(order.unit).map(|entity| units.get(entity))
        else {
            continue;
        };
        if owner.slot() != command.slot {
            continue;
        }
        let checked_order = match order.action {
            Action::Move { x, z } => walks.then_some(UnitOrder::Move { x, z }),
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
                    .and_then(|held| book.get(held.action))
                    .map(|action| action.kind.kind());
                matches!(kind, Some(ActionKind::Cast | ActionKind::Train))
                    .then_some(UnitOrder::Slot { slot, target })
            }
            Action::Learn { .. } => None,
        };
        let entity = index.get(order.unit).expect("a unit the index named");
        checked.0.extend(checked_order.map(|order| (entity, order)));
    }
}

/// The parts of a unit that an order reads and changes.
type Ordered = (
    &'static Position,
    Option<&'static SpawnPoint>,
    Option<&'static mut ActionSlots>,
    Option<&'static mut PathWalker>,
    Option<&'static mut Destination>,
);

/// Applies the tick's checked player orders, in input order, each as every order applies; a
/// player orders no reset.
fn apply_player_orders(
    bounds: Res<'_, Bounds>,
    mut checked: ResMut<'_, PlayerOrders>,
    mut units: Query<'_, '_, Ordered>,
) {
    for (entity, order) in checked.0.drain(..) {
        let (&at, spawn, slots, walker, destination) =
            units.get_mut(entity).expect("a checked order's unit");
        let ordered = OrderedUnit {
            at,
            spawn: spawn.map(|spawn| spawn.get()),
            slots,
            walker,
            destination,
        };
        let resets = order.apply(ordered, &bounds);
        debug_assert!(!resets, "a player orders no reset");
    }
}

/// Applies each learn order the tick's inputs give, in input order, so a second learn in a tick
/// sees the point the first spent: to a unit its player controls, dead or not, the next rank of
/// the action in the slot, for a point, when the unit's level reaches the one the slot's kind
/// gives that rank. It changes nothing under way. An order that fails a check is dropped: a
/// client can send anything.
fn learn_ranks(
    inputs: Res<'_, TickInputs>,
    index: Res<'_, EntityIndex>,
    (book, kinds): (Res<'_, ActionBook>, Res<'_, SlotKinds>),
    mut units: Query<'_, '_, (&Owner, &mut ActionSlots, &mut Points, &Level)>,
) {
    for command in inputs.commands(Order::CAPABILITY) {
        let Some(Order {
            unit,
            action: Action::Learn { slot },
        }) = Order::decode(command.body)
        else {
            continue;
        };
        let Some(Ok((owner, mut slots, mut points, &level))) =
            index.get(unit).map(|entity| units.get_mut(entity))
        else {
            continue;
        };
        let Some(held) = slots.slot(slot) else {
            continue;
        };
        let action = book
            .get(held.action)
            .expect("a slot's action is in the book");
        let next = held
            .rank
            .checked_add(1)
            .filter(|&next| action.has_rank(next));
        let learnable = next.is_some_and(|next| {
            kinds
                .level_of(held.kind, next)
                .is_none_or(|needed| needed <= level)
        });
        if owner.slot() != command.slot || !learnable || points.get() == 0 {
            continue;
        }
        points.spend();
        slots.learn(slot);
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
            batch
                .world()
                .entity_mut(entity)
                .insert(NextThink::new(next));
        }
    });
}

/// Sends each path walker with no attack target, on its path, to the waypoint it walks to, and on
/// to the next once the waypoint is within its body, or it stands on the waypoint with no body:
/// walkers that push each other never stand on one point. A walker that chased a target walks back
/// to where it left its path.
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
        if walker.left() || slots.is_some_and(|slots| slots.attack_target().is_some()) {
            continue;
        }
        let path = path.get();
        let mut waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
        if waypoint.is_some_and(|at| position.within_ground(at, Body::radius_of(body))) {
            walker.advance();
            waypoint = paths.waypoint(path, walker.next(), walker.walks_from());
        }
        walk_to(&mut destination, route, waypoint);
    }
}

/// Walks each unit that can move to its attack target while out of the range of the weapon it
/// attacks it with, and stops it in range or in its windup. A unit whose target is gone, dead, no
/// longer an enemy or one no weapon of it selects drops it and stops.
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
        if slots.attacking().is_some() {
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
                walk_to(&mut destination, route, None);
            }
            Some((unit, Range::Meters(range)))
                if !targets.reaches(position, Body::radius_of(body), range, &unit) =>
            {
                walk_to(&mut destination, route, Some(unit.pos));
            }
            Some(_) => walk_to(&mut destination, route, None),
        }
    }
}

/// Sets where a unit walks, leaving a destination that does not change untouched: a write marks
/// it changed, and an avatar's destination replicates. A unit whose `route` arrived short of
/// `target` stays where it stands, with no destination, until the static bodies change.
fn walk_to(
    destination: &mut Mut<'_, Destination>,
    route: Option<&Route>,
    target: Option<Position>,
) {
    let arrived =
        target.is_some_and(|target| route.is_some_and(|route| route.arrived_short_of(target)));
    if !arrived {
        destination.set_if_neq(Destination::to(target));
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
