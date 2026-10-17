use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::system::{Local, ParamSet, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::{
    ActionCheck, ActionKind, ActionProgress, ActionSlots, ByAction, Caster, Payer, Purse,
    SlotCommit, SlotTimes, StartChecked,
};
use campfire_declare::{ByType, RoleForm};
use campfire_math::{Num, Vec3};
use campfire_players::{PlayerResources, PlayerRows, PlayerRowsMut};
use campfire_sim::{IdAllocator, Keyed, Ordered, Position, SimTick, SimWorld, StableId};
use campfire_stats::{PlayerModifiers, PoolBook, UnitPools};
use campfire_units::{
    Body, Dead, Destination, Facing, MapBook, OpenCellAsk, OpenCellRun, Owner, SpawnAt, Spawner,
};
use campfire_values::{Motion, Relation, Team, UnitTags, UnitType};

use crate::frame_point::FramePoint;
use crate::gather_spec::GatherSpec;
use crate::gatherer::{GatherOrder, GatherStep, Gatherer, NodeAt};
use crate::holdings::Holdings;
use crate::node_book::NodeBook;
use crate::production_books::ProductionBooks;
use crate::production_data::ProductionData;
use crate::rally::Rally;
use crate::rally_target::RallyTarget;
use crate::supply::{Counted, Supply};
use crate::train_queue::{Queued, TrainQueue};
use crate::train_spec::TrainSpec;

/// Where a producer's trained units come out and where they go, on the ground plane: its exit
/// point, and the rally point its player set, else its default one; none of a producer that
/// names neither.
#[derive(Debug, Clone, Copy)]
struct TrainPlaces {
    exit: Option<Vec3>,
    rally: Option<Vec3>,
}

/// The parts of a living unit a train it was ordered reads and changes.
type Training = (
    Entity,
    &'static StableId,
    &'static UnitType,
    (&'static ActionSlots, &'static mut ActionProgress),
    &'static mut SlotTimes,
    &'static mut TrainQueue,
    UnitPools,
    Option<&'static Owner>,
    Option<&'static UnitTags>,
);

/// The trains units were ordered: their start in Act, and their finish, which spawns the unit
/// trained.
#[derive(Debug)]
pub struct Trains;

impl Trains {
    /// Starts each ordered train, in Act, by its unit's stable id: one whose unit's tags do not
    /// block production that passes the core's checks, finds a place in its unit's queue, meets
    /// its `requires` and, with supply, finds room under its player's cap, pays its whole cost, in
    /// pools and player resources, goes on cooldown, and joins the queue, where it holds its
    /// supply (04-capabilities/production.md, Tags block production). The order ends either way,
    /// and the unit stays free. Supply and the units a requirement finds are counted once, as the first
    /// train comes to them. A predicting client's form `F` pays nothing, as the unit's pools and
    /// its player's resources are the server's.
    #[expect(
        clippy::too_many_arguments,
        reason = "a system takes each resource and query it reads"
    )]
    pub(super) fn start_trains<F: RoleForm>(
        tick: Res<'_, SimTick>,
        books: ProductionBooks<'_>,
        held_modifiers: PlayerRows<'_, '_, PlayerModifiers>,
        life: Option<Res<'_, PoolBook>>,
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
        mut holdings: ResMut<'_, Holdings>,
    ) {
        let ProductionBooks {
            actions: book,
            producers,
            requirements,
            trains: specs,
            costs,
            rules,
        } = books;
        let now = tick.start();
        let trains = {
            let training = units.p1();
            let ordered = training
                .iter()
                .filter_map(|(entity, &id, _, (slots, progress), ..)| {
                    progress
                        .ordered(slots, &book, ActionKind::Train)
                        .map(|_| Keyed { id, entity })
                });
            order.sort(ordered)
        };
        if trains.is_empty() {
            return;
        }
        {
            let counted = units.p0();
            if let Some(rules) = rules.0 {
                supply.count(&costs, rules, counted.iter());
            }
            holdings.read(counted.iter());
        }
        let mut training = units.p1();
        for &Keyed { entity, .. } in trains {
            let (
                _,
                _,
                &unit_type,
                (slots, mut progress),
                mut times,
                mut queue,
                pools,
                held_by,
                tags,
            ) = training.get_mut(entity).expect("a unit in the order");
            let pools = pools.of(life.as_deref().and_then(PoolBook::life));
            let ordered = progress
                .ordered(slots, &book, ActionKind::Train)
                .expect("an ordered train");
            progress.stop();
            let purse = Purse::of(pools.read(), resources.of(held_by));
            let owner = held_by.map(|owner| owner.slot());
            let no_target = |_| Relation::Friendly;
            let started = times.start(
                ActionCheck {
                    book: &book,
                    now,
                    caster: Caster::of(tags, Motion::Own, None),
                    slots,
                    purse,
                    aim: ordered,
                    relation: no_target,
                    living: |_| None,
                },
                None,
            );
            let Some(StartChecked { checked, .. }) = started else {
                continue;
            };
            let capacity = producers.get(unit_type).map(|production| production.queue);
            if !capacity.is_some_and(|capacity| queue.has_room(capacity)) {
                continue;
            }
            let modifiers = held_modifiers.of(held_by);
            if !holdings.meet(owner, modifiers, requirements.of(checked.id)) {
                continue;
            }
            let made = specs
                .get(checked.id)
                .expect("a train's action trains")
                .unit_type;
            let cost = costs.cost(made);
            let counts = rules.0.is_some().then_some(owner).flatten();
            if counts.is_some_and(|owner| !supply.has_room(owner, cost)) {
                continue;
            }
            let values = checked.values;
            let paid = checked.action.resource_cost(checked.rank);
            if !F::PREDICTS {
                let mut paying = resources.of_mut(held_by);
                let payer = Payer::of(pools, paying.as_deref_mut());
                payer.pay(&values.cost, paid);
            }
            let queued = Queued {
                action: checked.id,
                rank: checked.rank,
                time: values.windup,
                paid: u8::try_from(paid.len()).expect("a cost names at most a mode's resources"),
            };
            times.commit(SlotCommit {
                slot: ordered.slot,
                now,
                ready_at: now.after(values.cooldown),
                charges: values.charges,
                toggle: None,
            });
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
        for &Keyed { entity, .. } in due {
            while let Some(head) = world
                .get::<TrainQueue>(entity)
                .and_then(|queue| queue.done(now))
            {
                let trains = world.resource::<ByAction<TrainSpec>>();
                let unit_type = trains
                    .get(head.action)
                    .expect("a train queues only trains")
                    .unit_type;
                let producer = world.entity(entity);
                let team = *producer.get::<Team>().expect("a producer has a team");
                let owner = producer.get::<Owner>().map(|owner| owner.slot());
                let places = TrainPlaces::of(world, entity);
                let pos = places.spawn_place(world, entity, unit_type);
                let id = world.resource_mut::<IdAllocator>().allocate();
                let at = SpawnAt {
                    id,
                    unit_type,
                    team,
                    pos,
                    owner,
                    angle: Num::ZERO,
                };
                let trained = Spawner::spawn(world, at, ());
                let gathers = Self::gather_at_rally(world, entity, trained);
                if gathers.is_none()
                    && let Some(to) = places.rally
                    && let Some(mut destination) = world.get_mut::<Destination>(trained)
                {
                    let to = Vec3::new(to.x, pos.y, to.z);
                    destination.set_if_neq(Destination::to(Some(to)));
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
        let RallyTarget::Unit(node) = world.get::<Rally>(entity)?.get()? else {
            return None;
        };
        let target = world.entity_ref_of(node)?;
        let resource = world
            .resource::<NodeBook>()
            .resource(*target.get::<UnitType>()?)?;
        let pos = *target.get::<Position>()?;
        let slots = world.get::<ActionSlots>(trained)?;
        let gathers = world.resource::<ByAction<GatherSpec>>();
        let slot = slots.indexed().find_map(|(slot, action_slot)| {
            let spec = gathers.get(action_slot.action?)?;
            (spec.resource == resource).then_some(slot)
        })?;
        Some(GatherOrder {
            slot,
            node: NodeAt {
                node,
                pos: pos.get(),
            },
            step: GatherStep::Ordered,
        })
    }

    /// Where the units the producer of `entity` trains go: its rally point, or where the unit it
    /// rallies to stands; `None` with none, or a unit gone or dead.
    fn rally_point(world: &World, entity: Entity) -> Option<Vec3> {
        let producer = world.entity(entity);
        let pos = producer.get::<Position>().expect("a producer stands").get();
        match producer.get::<Rally>()?.get()? {
            RallyTarget::Point { x, z } => {
                Some(world.resource::<MapBook>().bounds.ground_point([x, z], pos))
            }
            RallyTarget::Unit(id) => {
                let unit = world.entity_ref_of(id)?;
                let unit_pos = *unit.get::<Position>()?;
                (!unit.contains::<Dead>()).then_some(unit_pos.get())
            }
        }
    }
}

impl TrainPlaces {
    /// The places of the producer of `entity`: its exit point and its default rally point, of
    /// its type's `production`, each turned with it and taken into the bounds; and its rally
    /// point, where one is set to a point or a living unit, in place of the default.
    fn of(world: &World, entity: Entity) -> Self {
        let producer = world.entity(entity);
        let pos = producer.get::<Position>().expect("a producer stands").get();
        let angle = producer
            .get::<Facing>()
            .map_or(Num::ZERO, |facing| facing.degrees());
        let unit_type = *producer.get::<UnitType>().expect("a producer has a type");
        let data = world.resource::<ByType<ProductionData>>().get(unit_type);
        let bounds = &world.resource::<MapBook>().bounds;
        let place = |point: Option<FramePoint>| {
            let at = point?.placed(pos, angle);
            Some(bounds.ground_point([at.x, at.z], at))
        };
        Self {
            exit: place(data.and_then(|data| data.exit)),
            rally: Trains::rally_point(world, entity).or_else(|| place(data?.rally)),
        }
    }

    /// Where a unit of `unit_type` the producer of `entity` trains spawns: at the center of the
    /// cell nearest the exit point, or with none the point of the producer's body nearest the
    /// rally point, among the cells its walker may stand in, as a teleport finds its cell; with
    /// no such cell, or a unit that does not walk, at the exit point, or the producer's
    /// position. With neither point, at the producer's position. A box's nearest point rounds to
    /// the nearest bit; a circle's is a step of its radius toward the rally point, as a walker
    /// steps.
    fn spawn_place(self, world: &mut World, entity: Entity, unit_type: UnitType) -> Vec3 {
        let producer = world.entity(entity);
        let pos = producer.get::<Position>().expect("a producer stands").get();
        let near = match (self.exit, self.rally) {
            (Some(exit), _) => exit,
            (None, Some(to)) => Body::shape_of(producer.get::<Body>()).nearest_point(pos, to),
            (None, None) => return pos,
        };
        let fallback = self.exit.unwrap_or(pos);
        let Some(&OpenCellRun(run)) = world.get_resource::<OpenCellRun>() else {
            return fallback;
        };
        world
            .run_system_with(run, OpenCellAsk { unit_type, near })
            .expect("the open cell's search runs on any world")
            .unwrap_or(fallback)
    }
}
