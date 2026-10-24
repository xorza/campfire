use bevy_ecs::change_detection::Mut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{QueryState, Without};
use bevy_ecs::system::{Local, ParamSet, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::{
    ActionCheck, ActionKind, ActionProgress, ActionSlots, Caster, Payer, ProgressIndexes, Purse,
    SlotCommit, SlotTimes, StartChecked,
};
use campfire_declarations::{ById, RoleForm};
use campfire_math::{Num, Vec3};
use campfire_players::{PlayerResources, PlayerRows, PlayerRowsMut};
use campfire_sim::{
    DueIndex, DueUnit, EntityIndex, IdAllocator, Keyed, Ordered, Position, SimTick, SimWorld,
    StableId, UnitDues,
};
use campfire_stats::{PlayerModifiers, PoolBook, UnitPools};
use campfire_units::{
    Body, Dead, Facing, GroundPoint, Leader, LivingSpawns, MapBook, OpenCellAsk, OpenCellRun,
    Owner, SpawnAt, Spawner, Walk,
};
use campfire_values::{ActionId, Motion, Relation, Team, UnitTags, UnitType};

use crate::frame_point::FramePoint;
use crate::gather_loop::GatherLoop;
use crate::gather_spec::GatherSpec;
use crate::gatherer::{GatherOrder, GatherStep, Gatherer, NodeAt};
use crate::holdings::{Holdings, OwnedUnits};
use crate::node_book::NodeBook;
use crate::producer_spec::ProducerSpec;
use crate::production_books::{ProductionBooks, TrainAdmission, TrainAsk};
use crate::rally::Rally;
use crate::rally_target::RallyTarget;
use crate::train_queue::{QueueIndex, Queued, TrainQueue};
use crate::train_spec::TrainSpec;
use crate::trained_units::{Trained, TrainedUnits};

/// Where a producer's trained units come out and where they go, on the ground plane: its exit
/// point, and the rally point its player set, else its default one; none of a producer that
/// names neither.
#[derive(Debug, Clone, Copy)]
struct TrainPoints {
    exit: Option<Vec3>,
    rally: Option<Vec3>,
}

/// Where a unit leaves its producer, as the spawner takes it, and the point it then rallies to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExitSpawn {
    pub(crate) at: SpawnAt,
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
    /// Starts each ordered train of the units the index of the orders holds, in Act, by its
    /// unit's stable id: one whose unit's tags do not block production that passes the core's
    /// checks, finds a place in its unit's queue, meets
    /// its `requires` and, with supply, finds room under its player's cap, pays its whole cost, in
    /// pools and player resources, goes on cooldown, and joins the queue, where it holds its
    /// supply (04-capabilities/production.md, Tags block production). The order ends either way,
    /// and the unit stays free. The holdings follow the units as each run starts, so the supply
    /// and the units a requirement finds are those of the match as it stands, and each train that
    /// joins holds its supply at once. A predicting client's form `F` pays nothing, as the unit's
    /// pools and its player's resources are the server's.
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
        mut units: ParamSet<'_, '_, (OwnedUnits<'_, '_>, Query<'_, '_, Training, Without<Dead>>)>,
        mut held: Local<'_, Vec<DueUnit>>,
        mut holdings: ResMut<'_, Holdings>,
        (mut indexes, mut queues): (ProgressIndexes<'_>, QueueIndex<'_>),
    ) {
        let book = &books.actions;
        let now = tick.start();
        holdings.follow(&units.p0());
        held.clear();
        indexes.orders(&mut held);
        let training = units.p1();
        held.retain(|unit| {
            training
                .get(unit.entity)
                .is_ok_and(|(_, _, _, (slots, progress), ..)| {
                    progress.ordered(slots, book, ActionKind::Train).is_some()
                })
        });
        if held.is_empty() {
            return;
        }
        let mut training = units.p1();
        for &DueUnit { id, entity } in &*held {
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
                .ordered(slots, book, ActionKind::Train)
                .expect("an ordered train");
            progress.end(&mut indexes.of(id, entity));
            let purse = Purse::of(pools.read(), resources.of(held_by));
            let owner = held_by.map(|owner| owner.player_slot());
            let no_target = |_| Relation::Friendly;
            let started = times.start(
                ActionCheck {
                    book,
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
            let ask = TrainAsk {
                unit_type,
                queue: &queue,
                owner,
                modifiers: held_modifiers.of(held_by),
                action: checked.id,
            };
            let admission = books.admits(&holdings, ask);
            if admission == TrainAdmission::Refused {
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
            queue.push(queued, paid, now, &mut queues.of(id, entity));
            if let TrainAdmission::Holds { owner, supply } = admission {
                holdings.reserve(entity, owner, supply);
            }
        }
    }

    /// Spawns each train whose time ended this tick, of the units the index of the queues holds
    /// due, by its unit's stable id, then its queue's order, out of its unit's exit as `leave`
    /// sends it; the next in a queue starts in the same tick. Each unit trained joins the trained
    /// units, whose `on_unit_trained` the mode runs. A dead unit's queue waits.
    pub(super) fn finish_trains(
        world: &mut World,
        producers: &mut QueryState<&TrainQueue, Without<Dead>>,
        (mut order, mut taken): (Local<'_, Ordered>, Local<'_, Vec<DueUnit>>),
    ) {
        let now = world.resource::<SimTick>().end();
        taken.clear();
        world
            .resource_mut::<DueIndex<TrainQueue>>()
            .take_due(now, &mut taken);
        if taken.is_empty() {
            return;
        }
        let done = taken.iter().filter(|unit| {
            let queue = producers.get(world, unit.entity);
            queue.is_ok_and(|queue| queue.done(now).is_some())
        });
        let due = order.sort(done.map(|&DueUnit { id, entity }| Keyed { id, entity }));
        world.resource_scope(|world, mut index: Mut<'_, DueIndex<TrainQueue>>| {
            for unit in &*taken {
                if let Ok(queue) = producers.get(world, unit.entity)
                    && queue.done(now).is_none()
                {
                    UnitDues::new(&mut index, unit.id, unit.entity).push(queue.head_done());
                }
            }
        });
        for &Keyed {
            id: trainer,
            entity,
        } in due
        {
            while let Some(head) = world
                .get::<TrainQueue>(entity)
                .and_then(|queue| queue.done(now))
            {
                let trains = world.resource::<ById<ActionId, TrainSpec>>();
                let spec = trains.get(head.action).expect("a train queues only trains");
                let unit = Self::leave(world, entity, spec.unit_type);
                world.resource_mut::<TrainedUnits>().0.extend([Trained {
                    unit,
                    producer: trainer,
                }]);
                world.resource_scope(|world, mut index: Mut<'_, DueIndex<TrainQueue>>| {
                    let mut queue = world.get_mut::<TrainQueue>(entity).expect("a producer");
                    let dues = &mut UnitDues::new(&mut index, trainer, entity);
                    queue.remove(0, now, dues);
                });
            }
        }
    }

    /// Spawns each unit type the producer of `entity` frees, in its list's order, out of its exit
    /// as `leave` sends it: the server's, as it stands whole (04-capabilities/production.md, A
    /// building may come with its units).
    pub(crate) fn free_units(world: &mut World, entity: Entity) {
        let unit_type = *world
            .get::<UnitType>(entity)
            .expect("a producer has a type");
        let producers = world.resource::<ById<UnitType, ProducerSpec>>().clone();
        let free = producers.get(unit_type).map(|spec| &*spec.free);
        for &freed in free.into_iter().flatten() {
            Self::leave(world, entity, freed);
        }
    }

    /// Spawns a unit of `unit_type` out of the exit of the producer of `entity`, of its team and
    /// player, through the mode's spawner, at its spawn point, and sends it on as `send_on` does.
    /// Its id.
    fn leave(world: &mut World, entity: Entity, unit_type: UnitType) -> StableId {
        let exit = Self::exit_spawn(world, entity, unit_type);
        let unit = Spawner::spawn(world, exit.at, ());
        Self::send_on(world, entity, unit, exit);
        exit.at.id
    }

    /// Where a unit of `unit_type` leaves the producer of `entity`: of its team and player, at
    /// its spawn point, with a new id, and the point it rallies to.
    pub(crate) fn exit_spawn(world: &mut World, entity: Entity, unit_type: UnitType) -> ExitSpawn {
        let points = TrainPoints::of(world, entity);
        let position = points.spawn_point(world, entity, unit_type);
        ExitSpawn {
            at: Self::spawn_at(world, entity, unit_type, position),
            rally: points.rally,
        }
    }

    /// Where a unit of `unit_type` buds for the spawner of `entity`, near `near`: of its team and
    /// player, at the centre of the cell nearest `near` that its walker may stand in, as a
    /// teleport finds its cell, or at `near` with no such cell, with a new id, rallying nowhere
    /// (04-capabilities/production.md, A later spawn buds from the nearest spawn).
    pub(crate) fn bud_spawn(
        world: &mut World,
        entity: Entity,
        unit_type: UnitType,
        near: Vec3,
    ) -> ExitSpawn {
        let position = TrainPoints::open_cell(world, unit_type, near).unwrap_or(near);
        ExitSpawn {
            at: Self::spawn_at(world, entity, unit_type, position),
            rally: None,
        }
    }

    /// A unit of `unit_type` of the team and the player of the unit of `entity`, at `position`,
    /// with a new id.
    fn spawn_at(world: &mut World, entity: Entity, unit_type: UnitType, position: Vec3) -> SpawnAt {
        let producer = world.entity(entity);
        let team = *producer.get::<Team>().expect("a producer has a team");
        let owner = producer.get::<Owner>().map(|owner| owner.player_slot());
        SpawnAt {
            id: world.resource_mut::<IdAllocator>().allocate(),
            unit_type,
            team,
            position,
            owner,
            angle: Num::ZERO,
            variation: None,
        }
    }

    /// Sends `unit`, which left the producer of `entity` by `exit`, on: a gatherer to the node
    /// the producer rallies to, by its first gather of the node's resource, else, of a producer
    /// that starts its gatherers, to the free node its first gather finds, either to be checked
    /// as the next tick's orders are; any other to the producer's rally point.
    pub(crate) fn send_on(world: &mut World, entity: Entity, unit: Entity, exit: ExitSpawn) {
        let gathers = world.get::<UnitType>(entity).is_some_and(|&producer_type| {
            let producers = world.resource::<ById<UnitType, ProducerSpec>>();
            producers
                .get(producer_type)
                .is_some_and(|spec| spec.gathers)
        });
        let gatherer = world.entity(unit).contains::<Gatherer>();
        let mut order = Self::gather_at_rally(world, entity, unit).filter(|_| gatherer);
        if order.is_none() && gatherer && gathers {
            order = world
                .run_system_cached_with(GatherLoop::seek, unit)
                .expect("the gather loop's search runs on a match that gathers");
        }
        if let Some(order) = order {
            Gatherer::set_in(world, unit, Some(order));
        } else if let Some(to) = exit.rally {
            let to = Vec3::new(to.x, exit.at.position.y, to.z);
            // A leader walks by its spawns (04-capabilities/production.md, A unit may lead its
            // spawns).
            let led = world.entity(unit).contains::<Leader>();
            let Some(spawns) = world.get::<LivingSpawns>(unit).filter(|_| led) else {
                Walk::go_to(world, unit, Some(to));
                return;
            };
            let spawns: Vec<StableId> = spawns.iter().copied().collect();
            for spawn in spawns {
                if let Some(entity) = world.resource::<EntityIndex>().get(spawn) {
                    Walk::go_to(world, entity, Some(to));
                }
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
        let position = *target.get::<Position>()?;
        let slots = world.get::<ActionSlots>(trained)?;
        let gathers = world.resource::<ById<ActionId, GatherSpec>>();
        let slot = slots.indexed().find_map(|(slot, action_slot)| {
            let spec = gathers.get(action_slot.action?)?;
            (spec.resource == resource).then_some(slot)
        })?;
        Some(GatherOrder {
            slot,
            node: NodeAt {
                node,
                position: position.get(),
            },
            step: GatherStep::Ordered,
        })
    }

    /// Where the units the producer of `entity` trains go: its rally point, or where the unit it
    /// rallies to stands; `None` with none, or a unit gone or dead.
    fn rally_point(world: &World, entity: Entity) -> Option<Vec3> {
        let producer = world.entity(entity);
        let position = producer.get::<Position>().expect("a producer stands").get();
        match producer.get::<Rally>()?.get()? {
            RallyTarget::Point(GroundPoint { x, z }) => Some(
                world
                    .resource::<MapBook>()
                    .bounds
                    .ground_point([x, z], position),
            ),
            RallyTarget::Unit(id) => {
                let unit = world.entity_ref_of(id)?;
                let unit_pos = *unit.get::<Position>()?;
                (!unit.contains::<Dead>()).then_some(unit_pos.get())
            }
        }
    }
}

impl TrainPoints {
    /// The points of the producer of `entity`: its exit point and its default rally point, of
    /// its type's `production`, each turned with it and taken into the bounds; and its rally
    /// point, where one is set to a point or a living unit, in place of the default.
    fn of(world: &World, entity: Entity) -> Self {
        let producer = world.entity(entity);
        let position = producer.get::<Position>().expect("a producer stands").get();
        let angle = producer
            .get::<Facing>()
            .map_or(Num::ZERO, |facing| facing.degrees());
        let unit_type = *producer.get::<UnitType>().expect("a producer has a type");
        let data = world
            .resource::<ById<UnitType, ProducerSpec>>()
            .get(unit_type);
        let bounds = &world.resource::<MapBook>().bounds;
        let point_of = |point: Option<FramePoint>| {
            let at = point?.placed(position, angle);
            Some(bounds.ground_point([at.x, at.z], at))
        };
        Self {
            exit: point_of(data.and_then(|data| data.exit)),
            rally: Trains::rally_point(world, entity).or_else(|| point_of(data?.rally)),
        }
    }

    /// Where a unit of `unit_type` the producer of `entity` trains spawns: at the center of the
    /// cell nearest the exit point, or with none the point of the producer's body nearest the
    /// rally point, among the cells its walker may stand in, as a teleport finds its cell; with
    /// no such cell, or a unit that does not walk, at the exit point, or the producer's
    /// position. With neither point, at the producer's position. A box's nearest point rounds to
    /// the nearest bit; a circle's is a step of its radius toward the rally point, as a walker
    /// steps.
    fn spawn_point(self, world: &mut World, entity: Entity, unit_type: UnitType) -> Vec3 {
        let producer = world.entity(entity);
        let position = producer.get::<Position>().expect("a producer stands").get();
        let near = match (self.exit, self.rally) {
            (Some(exit), _) => exit,
            (None, Some(to)) => Body::shape_of(producer.get::<Body>()).nearest_point(position, to),
            (None, None) => return position,
        };
        let fallback = self.exit.unwrap_or(position);
        Self::open_cell(world, unit_type, near).unwrap_or(fallback)
    }

    /// The centre of the cell nearest `near` that a walker of `unit_type` may stand in; none in a
    /// match of no such search, or with no such cell.
    fn open_cell(world: &mut World, unit_type: UnitType, near: Vec3) -> Option<Vec3> {
        let &OpenCellRun(run) = world.get_resource::<OpenCellRun>()?;
        world
            .run_system_with(run, OpenCellAsk { unit_type, near })
            .expect("the open cell's search runs on any world")
    }
}
