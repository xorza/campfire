use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState, ROQueryItem, Without};
use bevy_ecs::schedule::{IntoScheduleConfigs, Schedule, SystemSet};
use bevy_ecs::system::{Local, ParamSet, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{
    EntityIndex, IdAllocator, Keyed, Ordered, Position, SimSet, SimTick, StableId, StateRegistry,
};

use crate::actions::ActionsSet;
use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::{ActionSlots, InProgress, OrderPhase, SlotAim};
use crate::actions::kind_spec::KindSpec;
use crate::actions::purse::{Payer, Purse};
use crate::navigation::body_index::BodyIndex;
use crate::navigation::destination::Destination;
use crate::navigation::walker::Walker;
use crate::navigation::{Navigation, NavigationSet};
use crate::players::player_resources::PlayerResources;
use crate::production::build_specs::BuildSpecs;
use crate::production::builder::Builder;
use crate::production::construction::Construction;
use crate::production::holdings::{Held, Holdings};
use crate::production::production_column::ProductionColumn;
use crate::production::production_data::ProductionData;
use crate::production::rally::Rally;
use crate::production::rally_target::RallyTarget;
use crate::production::requirements::Requirements;
use crate::production::site::Site;
use crate::production::supply::{CountedUnit, Supply};
use crate::production::supply_costs::SupplyCosts;
use crate::production::supply_rules::SupplyRules;
use crate::production::train_queue::{Queued, TrainQueue};
use crate::stats::StatsSet;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::pools::Pools;
use crate::units::body::Body;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::row_fill::RowFill;
use crate::units::script_view::View;
use crate::units::spawner::{SpawnAt, Spawner};
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::attitude::Attitude;
use crate::values::bounds::Bounds;
use crate::values::shape::Shape;

pub(crate) mod build_specs;
pub(crate) mod build_target;
pub(crate) mod builder;
pub(crate) mod construction;
pub(crate) mod holdings;
pub(crate) mod placement;
pub(crate) mod production_api;
pub(crate) mod production_column;
pub(crate) mod production_data;
pub(crate) mod rally;
pub(crate) mod rally_target;
pub(crate) mod requirements;
pub(crate) mod site;
pub(crate) mod supply;
pub(crate) mod supply_costs;
pub(crate) mod supply_data;
pub(crate) mod supply_rules;
pub(crate) mod train_queue;

/// The `production` capability: units that train others, through a queue, within their player's
/// supply and requirements.
#[derive(Debug)]
pub struct Production;

/// The systems of `production`, for the mode to order its own against.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ProductionSet {
    /// In `SimSet::Inputs`: the build orders that changed are checked, after the orders apply.
    CheckBuilds,
    /// In `SimSet::Mode`: the trains whose time ended spawn.
    Finish,
}

/// The parts of a unit supply counts, dead or not.
type Counted = (
    &'static UnitType,
    &'static Owner,
    Has<Dead>,
    Option<&'static TrainQueue>,
    Has<Site>,
);

/// The parts of a living unit a train it was ordered reads and changes.
type Training = (
    Entity,
    &'static StableId,
    &'static UnitType,
    &'static mut ActionSlots,
    &'static mut TrainQueue,
    Option<&'static mut Pools>,
    Option<&'static Owner>,
);

/// The parts of a unit production adds to its row of the script view.
type RowParts = (
    Option<&'static Owner>,
    Option<&'static UnitType>,
    Has<Dead>,
    Option<&'static TrainQueue>,
    Has<Site>,
);

impl Production {
    /// Adds production to a match: in Act, after the other orders start, ordered trains pass their
    /// checks, pay, and join their unit's queue; in Mode, before the mode's hooks, the trains
    /// whose time ended spawn. With navigation, which tests a box for room, construction too: in
    /// Inputs, the build orders that changed are checked; in Act, after the trains, builds start;
    /// in Mode, before the trains finish, sites grow.
    pub fn install(world: &mut World, schedule: &mut Schedule, registry: &mut StateRegistry) {
        world.insert_resource(ByType::<ProductionData>::default());
        world.insert_resource(Requirements::default());
        world.insert_resource(SupplyCosts::default());
        world.insert_resource(BuildSpecs::default());
        let view = world.non_send::<View>().clone();
        view.add_column(ProductionColumn::default());
        view.add_source::<RowParts, _>(world, fill_row);
        schedule.add_systems((
            start_trains.in_set(SimSet::Act).after(ActionsSet::Start),
            Production::finish_trains
                .in_set(SimSet::Mode)
                .in_set(ProductionSet::Finish),
        ));
        // A build places a box, which only navigation tests for room.
        if world.contains_resource::<BodyIndex>() {
            schedule.add_systems((
                Construction::check_builds
                    .in_set(SimSet::Inputs)
                    .in_set(ProductionSet::CheckBuilds)
                    .after(StatsSet::Regenerate)
                    .after(NavigationSet::TrackStatics)
                    .after(ActionsSet::HoldAtInputs),
                (Construction::forget_dead_builds, Construction::start_builds)
                    .chain()
                    .in_set(SimSet::Act)
                    .after(start_trains),
                Construction::progress_sites
                    .in_set(SimSet::Mode)
                    .before(ProductionSet::Finish),
            ));
        }
        registry.register_component::<TrainQueue>();
        registry.register_component::<Rally>();
        registry.register_component::<Builder>();
        registry.register_component::<Site>();
    }

    /// Spawns each train whose time ended this tick, of its unit's team and player, by its unit's
    /// stable id, then its queue's order, through the mode's spawner, at its spawn place; the
    /// next in a queue starts in the same tick, and a unit trained toward a rally point is
    /// ordered there. A dead unit's queue waits.
    fn finish_trains(
        world: &mut World,
        producers: &mut QueryState<(Entity, &StableId, &TrainQueue), Without<Dead>>,
        mut order: Local<'_, Ordered>,
    ) {
        let now = world.resource::<SimTick>().end();
        let done = producers
            .iter(world)
            .filter(|(.., queue)| queue.done(now).is_some())
            .map(|(entity, &id, _)| Keyed { id, entity });
        let due = order.sort(done);
        if due.is_empty() {
            return;
        }
        let spawner = world.non_send::<Spawner>().clone();
        for &Keyed { entity, .. } in due {
            while let Some(head) = world
                .get::<TrainQueue>(entity)
                .and_then(|queue| queue.done(now))
            {
                let action = world.resource::<ActionBook>().get(head.action);
                let Some(KindSpec::Train(unit_type)) = action.map(|action| action.kind) else {
                    panic!("a train queues only trains");
                };
                let producer = world.entity(entity);
                let team = *producer.get::<Team>().expect("a producer has a team");
                let owner = producer.get::<Owner>().map(|owner| owner.slot());
                let rally = Production::rally_point(world, entity);
                let pos = Production::spawn_place(world, entity, unit_type, rally);
                let id = world.resource_mut::<IdAllocator>().allocate();
                let at = SpawnAt {
                    id,
                    unit_type,
                    team,
                    pos,
                    angle: Num::ZERO,
                };
                let trained = spawner.spawn(world, at, owner);
                if let Some(to) = rally
                    && let Some(mut destination) = world.get_mut::<Destination>(trained)
                {
                    let to = Vec3::new(to.get().x, pos.get().y, to.get().z);
                    destination.set(Position::new(to));
                }
                let mut queue = world.get_mut::<TrainQueue>(entity).expect("a producer");
                queue.remove(0, now);
            }
        }
    }

    /// Where the units the producer of `entity` trains go: its rally point, or where the unit it
    /// rallies to stands; `None` with none, or a unit gone or dead.
    fn rally_point(world: &World, entity: Entity) -> Option<Position> {
        let producer = world.entity(entity);
        let at = *producer.get::<Position>().expect("a producer stands");
        match producer.get::<Rally>()?.get() {
            RallyTarget::Point { x, z } => Some(Bounds::of(world).ground_point([x, z], at)),
            RallyTarget::Unit(id) => {
                let unit = world.entity(world.resource::<EntityIndex>().get(id)?);
                let pos = *unit.get::<Position>()?;
                (!unit.contains::<Dead>()).then_some(pos)
            }
        }
    }

    /// Where a unit of `unit_type` the producer of `entity` trains spawns: at the center of the
    /// cell nearest the point of the producer's body nearest `rally`, among the cells its walker
    /// may stand in, as a teleport finds its cell; at the producer's position with no rally
    /// point, or no such cell. A box's nearest point rounds to the nearest bit; a circle's is a
    /// step of its radius toward the rally point, as a walker steps.
    fn spawn_place(
        world: &World,
        entity: Entity,
        unit_type: UnitType,
        rally: Option<Position>,
    ) -> Position {
        let producer = world.entity(entity);
        let at = *producer.get::<Position>().expect("a producer stands");
        let walker = world
            .get_resource::<ByType<Walker>>()
            .and_then(|walkers| walkers.get(unit_type).copied());
        let (Some(to), Some(walker)) = (rally, walker) else {
            return at;
        };
        let near = match Body::shape_of(producer.get::<Body>()) {
            Shape::Box(body) => body.nearest_point(at, to),
            Shape::Circle(radius) if !at.within_ground(to, radius) => {
                let to = Vec3::new(to.get().x, at.get().y, to.get().z);
                let step = at.get().step_toward(to, radius);
                Position::new(step).expect("a step within a body's radius stays within the bound")
            }
            Shape::Circle(_) => to,
        };
        Navigation::open_cell(world, walker, near).unwrap_or(at)
    }

    /// The train `slots` hold ordered, not yet checked, whose action `book` says trains.
    fn ordered(slots: &ActionSlots, book: &ActionBook) -> Option<SlotAim> {
        let Some(InProgress::Order {
            aim,
            phase: OrderPhase::Ordered,
        }) = slots.in_progress()
        else {
            return None;
        };
        let slot = slots.slot(aim.slot)?;
        let trains = book.get(slot.action?)?.kind.kind() == ActionKind::Train;
        trains.then_some(aim)
    }
}

/// Fills a row of the script view with what the unit counts for of its player's supply.
fn fill_row(
    (owner, unit_type, dead, queue, site): ROQueryItem<'_, '_, RowParts>,
    fill: &mut RowFill<'_, ProductionColumn>,
) {
    let counted = unit_type
        .map(|&unit_type| fill.column.costs().unit(unit_type, dead, !site, queue))
        .unwrap_or_default();
    fill.column.push(owner.map(|owner| owner.slot()), counted);
}

/// Starts each ordered train, in Act, by its unit's stable id: one that passes the core's checks,
/// finds a place in its unit's queue, meets its `requires` and, with supply, finds room under its
/// player's cap, pays its whole cost, in pools and player resources, goes on cooldown, and joins
/// the queue, where it holds its supply. The order ends either way, and the unit stays free.
/// Supply and the units a requirement finds are counted once, as the first train comes to them.
fn start_trains(
    tick: Res<'_, SimTick>,
    (book, producers, requirements, costs): (
        Res<'_, ActionBook>,
        Res<'_, ByType<ProductionData>>,
        Res<'_, Requirements>,
        Res<'_, SupplyCosts>,
    ),
    (rules, held_modifiers): (
        Option<Res<'_, SupplyRules>>,
        Option<Res<'_, PlayerModifiers>>,
    ),
    mut resources: Option<ResMut<'_, PlayerResources>>,
    mut units: ParamSet<
        '_,
        '_,
        (
            Query<'_, '_, Counted>,
            Query<'_, '_, Training, Without<Dead>>,
        ),
    >,
    (mut order, mut supply, mut held): (
        Local<'_, Ordered>,
        Local<'_, Supply>,
        Local<'_, Vec<Held>>,
    ),
) {
    let now = tick.start();
    let trains = {
        let training = units.p1();
        let ordered = training.iter().filter_map(|(entity, &id, _, slots, ..)| {
            Production::ordered(slots, &book).map(|_| Keyed { id, entity })
        });
        order.sort(ordered)
    };
    if trains.is_empty() {
        return;
    }
    {
        let counted = units.p0();
        if let Some(rules) = rules.as_deref() {
            let units = counted
                .iter()
                .map(|(&unit_type, owner, dead, queue, site)| CountedUnit {
                    unit_type,
                    owner: owner.slot(),
                    dead,
                    complete: !site,
                    queue,
                });
            supply.count(&costs, *rules, units);
        }
        let complete = counted
            .iter()
            .filter(|&(_, _, dead, _, site)| !dead && !site);
        Held::collect(
            &mut held,
            complete.map(|(&unit_type, owner, ..)| Held {
                owner: owner.slot(),
                unit_type,
            }),
        );
    }
    let mut training = units.p1();
    for &Keyed { entity, .. } in trains {
        let (_, _, &unit_type, mut slots, mut queue, mut pools, owner) =
            training.get_mut(entity).expect("a unit in the order");
        let ordered = Production::ordered(&slots, &book).expect("an ordered train");
        slots.stop();
        let owner = owner.map(|owner| owner.slot());
        let purse = Purse {
            pools: pools.as_deref(),
            resources: resources.as_deref(),
            owner,
        };
        let no_target = |_| Attitude::Friendly;
        let Some(checked) = book.check(now, &slots, purse, ordered, no_target, |_| None) else {
            continue;
        };
        let capacity = producers.get(unit_type).map(|production| production.queue);
        if !capacity.is_some_and(|capacity| queue.has_room(capacity)) {
            continue;
        }
        let holdings = Holdings {
            units: &held,
            modifiers: held_modifiers.as_deref(),
        };
        if !holdings.meet(owner, requirements.of(checked.id)) {
            continue;
        }
        let KindSpec::Train(made) = checked.action.kind else {
            panic!("a train's action trains");
        };
        let cost = costs.cost(made);
        let counts = rules.is_some().then_some(owner).flatten();
        if counts.is_some_and(|owner| !supply.has_room(owner, cost)) {
            continue;
        }
        let values = checked.values;
        let paid = if owner.is_some() {
            checked.action.resource_cost(checked.rank)
        } else {
            &[]
        };
        let payer = Payer {
            pools: pools.as_deref_mut(),
            resources: resources.as_deref_mut(),
            owner,
        };
        payer.pay(&values.cost, checked.action.resource_cost(checked.rank));
        let queued = Queued {
            action: checked.id,
            rank: checked.rank,
            time: values.windup,
            paid: u8::try_from(paid.len()).expect("a cost names at most a mode's resources"),
        };
        slots.cool_down(ordered.slot, now.after(values.cooldown));
        queue.push(queued, paid, now);
        if let Some(owner) = counts {
            supply.reserve(owner, cost);
        }
    }
}

#[cfg(test)]
mod tests;
