use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, QueryState, Without};
use bevy_ecs::system::{Local, ParamSet, Query, Res};
use bevy_ecs::world::World;
use campfire_math::{Num, Vec3};
use campfire_sim::{IdAllocator, Keyed, Ordered, Position, SimTick, SimWorld, StableId};

use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::ActionSlots;
use crate::actions::kind_spec::KindSpec;
use crate::actions::payer::Payer;
use crate::actions::purse::Purse;
use crate::actions::slot_times::SlotTimes;
use crate::geometry::bounds::Bounds;
use crate::navigation::Navigation;
use crate::navigation::destination::Destination;
use crate::navigation::walker::Walker;
use crate::players::player_resources::PlayerResources;
use crate::players::player_rows::{PlayerRows, PlayerRowsMut};
use crate::production::gatherer::{GatherOrder, GatherStep, Gatherer, NodeAt};
use crate::production::held::Held;
use crate::production::holdings::Holdings;
use crate::production::node_book::NodeBook;
use crate::production::production_books::ProductionBooks;
use crate::production::rally::Rally;
use crate::production::rally_target::RallyTarget;
use crate::production::site::Site;
use crate::production::supply::{CountedUnit, Supply};
use crate::production::train_queue::{Queued, TrainQueue};
use crate::stats::life_pool::LifePool;
use crate::stats::player_modifiers::PlayerModifiers;
use crate::stats::unit_pools::UnitPools;
use crate::units::body::Body;
use crate::units::by_type::ByType;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::spawn_at::SpawnAt;
use crate::units::spawner::Spawner;
use crate::units::team::Team;
use crate::units::unit_type::UnitType;
use crate::values::relation::Relation;

/// The parts of a unit supply counts, dead or not.
pub(super) type Counted = (
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
    &'static mut SlotTimes,
    &'static mut TrainQueue,
    UnitPools,
    Option<&'static Owner>,
);

/// The trains units were ordered: their start in Act, and their finish, which spawns the unit
/// trained.
#[derive(Debug)]
pub(super) struct Trains;

impl Trains {
    /// Starts each ordered train, in Act, by its unit's stable id: one that passes the core's
    /// checks, finds a place in its unit's queue, meets its `requires` and, with supply, finds room
    /// under its player's cap, pays its whole cost, in pools and player resources, goes on
    /// cooldown, and joins the queue, where it holds its supply. The order ends either way, and the
    /// unit stays free. Supply and the units a requirement finds are counted once, as the first
    /// train comes to them.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    pub(super) fn start_trains(
        tick: Res<'_, SimTick>,
        books: ProductionBooks<'_>,
        held_modifiers: PlayerRows<'_, '_, PlayerModifiers>,
        life: Option<Res<'_, LifePool>>,
        mut resources: PlayerRowsMut<'_, '_, PlayerResources>,
        mut units: ParamSet<
            '_,
            '_,
            (
                Query<'_, '_, Counted>,
                Query<'_, '_, Training, Without<Dead>>,
            ),
        >,
        mut order: Local<'_, Ordered>,
        mut supply: Local<'_, Supply>,
        mut held: Local<'_, Vec<Held>>,
    ) {
        let ProductionBooks {
            actions: book,
            producers,
            requirements,
            costs,
            rules,
        } = books;
        let now = tick.start();
        let trains = {
            let training = units.p1();
            let ordered = training.iter().filter_map(|(entity, &id, _, slots, ..)| {
                slots
                    .ordered(&book, ActionKind::Train)
                    .map(|_| Keyed { id, entity })
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
            let (_, _, &unit_type, mut slots, mut times, mut queue, pools, held_by) =
                training.get_mut(entity).expect("a unit in the order");
            let pools = pools.of(life.as_deref().map(|life| life.0));
            let ordered = slots
                .ordered(&book, ActionKind::Train)
                .expect("an ordered train");
            slots.stop();
            let purse = Purse::of(pools.read(), resources.of(held_by));
            let owner = held_by.map(|owner| owner.slot());
            let no_target = |_| Relation::Friendly;
            let checked = book.check(now, (&slots, &times), purse, ordered, no_target, |_| None);
            let Some(checked) = checked else {
                continue;
            };
            let capacity = producers.get(unit_type).map(|production| production.queue);
            if !capacity.is_some_and(|capacity| queue.has_room(capacity)) {
                continue;
            }
            let holdings = Holdings { units: &held };
            let modifiers = held_modifiers.of(held_by);
            if !holdings.meet(owner, modifiers, requirements.of(checked.id)) {
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
            let paid = checked.action.resource_cost(checked.rank);
            let mut paying = resources.of_mut(held_by);
            let payer = Payer::of(pools, paying.as_deref_mut());
            payer.pay(&values.cost, paid);
            let queued = Queued {
                action: checked.id,
                rank: checked.rank,
                time: values.windup,
                paid: u8::try_from(paid.len()).expect("a cost names at most a mode's resources"),
            };
            times.cool_down(ordered.slot, now.after(values.cooldown));
            queue.push(queued, paid, now);
            if let Some(owner) = counts {
                supply.reserve(owner, cost);
            }
        }
    }

    /// Spawns each train whose time ended this tick, of its unit's team and player, by its unit's
    /// stable id, then its queue's order, through the mode's spawner, at its spawn place; the
    /// next in a queue starts in the same tick, and a unit trained toward a rally point is
    /// ordered there. A dead unit's queue waits.
    pub(super) fn finish_trains(
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
                let rally = Self::rally_point(world, entity);
                let pos = Self::spawn_place(world, entity, unit_type, rally);
                let id = world.resource_mut::<IdAllocator>().allocate();
                let at = SpawnAt {
                    id,
                    unit_type,
                    team,
                    pos,
                    angle: Num::ZERO,
                };
                let trained = spawner.spawn(world, at, owner);
                let gathers = Self::gather_at_rally(world, entity, trained);
                if gathers.is_none()
                    && let Some(to) = rally
                    && let Some(mut destination) = world.get_mut::<Destination>(trained)
                {
                    let to = Vec3::new(to.get().x, pos.get().y, to.get().z);
                    destination.set_if_neq(Destination::to(Position::new(to)));
                }
                if let (Some(order), Some(mut gatherer)) =
                    (gathers, world.get_mut::<Gatherer>(trained))
                {
                    gatherer.set(Some(order));
                }
                let mut queue = world.get_mut::<TrainQueue>(entity).expect("a producer");
                queue.remove(0, now);
            }
        }
    }

    /// The gather loop of `trained`, a unit the producer of `entity` trained, when the producer
    /// rallies to a node whose resource one of its gathers takes: by the first such gather in its
    /// slots, to be checked as the next tick's orders are.
    fn gather_at_rally(world: &World, entity: Entity, trained: Entity) -> Option<GatherOrder> {
        let RallyTarget::Unit(node) = world.get::<Rally>(entity)?.get() else {
            return None;
        };
        let target = world.entity_ref_of(node)?;
        let resource = world
            .resource::<NodeBook>()
            .resource(*target.get::<UnitType>()?)?;
        let pos = *target.get::<Position>()?;
        let slots = world.get::<ActionSlots>(trained)?;
        let book = world.resource::<ActionBook>();
        let slot = slots.indexed().find_map(|(slot, held)| {
            let action = book.get(held.action?)?;
            let KindSpec::Gather(spec) = action.kind else {
                return None;
            };
            (spec.resource == resource).then_some(slot)
        })?;
        Some(GatherOrder {
            slot,
            node: NodeAt { node, pos },
            step: GatherStep::Ordered,
        })
    }

    /// Where the units the producer of `entity` trains go: its rally point, or where the unit it
    /// rallies to stands; `None` with none, or a unit gone or dead.
    fn rally_point(world: &World, entity: Entity) -> Option<Position> {
        let producer = world.entity(entity);
        let pos = *producer.get::<Position>().expect("a producer stands");
        match producer.get::<Rally>()?.get() {
            RallyTarget::Point { x, z } => {
                Some(world.resource::<Bounds>().ground_point([x, z], pos))
            }
            RallyTarget::Unit(id) => {
                let unit = world.entity_ref_of(id)?;
                let unit_pos = *unit.get::<Position>()?;
                (!unit.contains::<Dead>()).then_some(unit_pos)
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
        let pos = *producer.get::<Position>().expect("a producer stands");
        let walker = world
            .get_resource::<ByType<Walker>>()
            .and_then(|walkers| walkers.get(unit_type).copied());
        let (Some(to), Some(walker)) = (rally, walker) else {
            return pos;
        };
        let near = Body::shape_of(producer.get::<Body>()).nearest_point(pos, to);
        Navigation::open_cell(world, walker, near).unwrap_or(pos)
    }
}
