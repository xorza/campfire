use std::iter;

use bevy_ecs::change_detection::Mut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, QueryFilter, QueryState, Without};
use bevy_ecs::system::{Local, ParamSet, Query, Res, ResMut};
use bevy_ecs::world::World;
use campfire_actions::{
    ActionCheck, ActionKind, ActionProgress, ActionSlots, Caster, Payer, ProgressIndexes, Purse,
    SlotCommit, SlotTimes, StartChecked,
};
use campfire_common::Tick;
use campfire_declarations::{ById, RoleForm};
use campfire_math::{Num, Vec3};
use campfire_players::{PlayerResources, PlayerRows, PlayerRowsMut};
use campfire_sim::{
    DueIndex, DueUnit, EntityIndex, IdAllocator, Keyed, Ordered, Position, SimTick, SimWorld,
    StableId, UnitDues,
};
use campfire_stats::{
    Modifiers, PlayerModifiers, PoolBook, Stats, StatsWorld, TypeUpgrades, UnitPools,
};
use campfire_units::{
    Body, Dead, Facing, ForcedMove, GroundPoint, Leader, Leaving, LivingSpawns, MapBook, MoveStep,
    OpenCellAsk, OpenCellRun, Owner, SpawnAt, Spawner, Walk,
};
use campfire_values::{ActionId, Motion, Relation, Team, UnitTags, UnitType};

use crate::door_times::DoorTimes;
use crate::doors::Doors;
use crate::frame_point::FramePoint;
use crate::gather_loop::GatherLoop;
use crate::gather_spec::GatherSpec;
use crate::gatherer::{GatherOrder, GatherStep, Gatherer, NodeAt};
use crate::held_places::HeldPlaces;
use crate::holdings::{Holdings, OwnedUnits};
use crate::made_at::MadeAt;
use crate::node_book::NodeBook;
use crate::producer_spec::ProducerSpec;
use crate::production_books::{ProductionBooks, TrainAdmission, TrainAsk};
use crate::rally::Rally;
use crate::rally_target::RallyTarget;
use crate::research_data::ResearchHolder;
use crate::research_spec::ResearchSpec;
use crate::train_queue::{QueueIndex, Queued, TrainQueue};
use crate::train_spec::TrainSpec;
use crate::trained_units::{Trained, TrainedUnits};

/// Where a producer's trained units come out and where they go, on the ground plane: its exit
/// point, the rally point its player set, else its default one, and its default one; none of a
/// producer that names neither.
#[derive(Debug, Clone, Copy)]
struct TrainPoints {
    exit: Option<Vec3>,
    rally: Option<Vec3>,
    natural: Option<Vec3>,
}

/// Where a unit leaves its producer, as the spawner takes it, the point it drives out to first,
/// for one made inside its producer, and the point it then rallies to.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ExitSpawn {
    pub(crate) at: SpawnAt,
    out_to: Option<Vec3>,
    rally: Option<Vec3>,
}

/// The kinds a producer's queue makes: a unit by a train, a modifier by a research.
const QUEUED: [ActionKind; 2] = [ActionKind::Train, ActionKind::Research];

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
    Option<&'static HeldPlaces>,
);

/// What a research's check of what is held or queued reads of each unit with a queue, dead or
/// living: its id, its owner, its queue, and the modifiers it carries.
pub(crate) type Queues = (
    &'static StableId,
    &'static UnitType,
    Option<&'static Owner>,
    &'static TrainQueue,
    Option<&'static Modifiers>,
);

/// The trains and researches units were ordered: their start in Act, and their finish, which
/// spawns the unit trained or gives the modifier researched.
#[derive(Debug)]
pub struct Trains;

impl Trains {
    /// Starts each ordered train or research of the units the index of the orders holds, in Act, by
    /// its unit's stable id: one whose unit's tags do not block production that passes the core's
    /// checks, finds a place in its unit's queue, meets its `requires`, as a train with supply
    /// finds room under its player's cap and as a research finds neither its modifier nor one its
    /// upgrades' conflicts hold back held or queued, pays its whole cost, in pools and player
    /// resources, goes on cooldown, and joins the queue, where a train holds its supply
    /// (04-capabilities/production.md, Tags block production). The order ends either way, and the
    /// unit stays free. The holdings follow the units as each run starts, so the supply and the
    /// units a requirement finds are those of the match as it stands, and each train that joins
    /// holds its supply at once. A predicting client's form `F` pays nothing, as the unit's pools
    /// and its player's resources are the server's.
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
                OwnedUnits<'_, '_>,
                Query<'_, '_, Training, Without<Dead>>,
                Query<'_, '_, Queues>,
            ),
        >,
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
                    QUEUED
                        .iter()
                        .any(|&kind| progress.ordered(slots, book, kind).is_some())
                })
        });
        if held.is_empty() {
            return;
        }
        for &DueUnit { id, entity } in &*held {
            let researching = {
                let training = units.p1();
                let (_, _, _, (slots, progress), ..) =
                    training.get(entity).expect("a unit in the order");
                let ordered = progress.ordered(slots, book, ActionKind::Research);
                ordered.map(|ordered| {
                    let held = slots.learned(ordered.slot).and_then(|held| held.action);
                    let action = held.expect("an ordered slot holds its action");
                    *books
                        .researches
                        .get(action)
                        .expect("a research has its spec")
                })
            };
            let researched = researching.is_some_and(|spec| {
                let queues = units.p2();
                let check = ResearchCheck {
                    players: &held_modifiers,
                    researches: &books.researches,
                    upgrades: books.upgrades.as_deref(),
                    queues: &queues,
                };
                check.refuses(spec, id, entity)
            });
            let mut training = units.p1();
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
                places,
            ) = training.get_mut(entity).expect("a unit in the order");
            let pools = pools.of(life.as_deref().and_then(PoolBook::life));
            let kind = if researching.is_some() {
                ActionKind::Research
            } else {
                ActionKind::Train
            };
            let ordered = progress
                .ordered(slots, book, kind)
                .expect("an ordered train or research");
            progress.end(&mut indexes.of(id, entity));
            let purse = Purse::of(pools.read(), resources.of(held_by));
            let owner = held_by.map(|owner| owner.player_slot());
            let no_target = |_| Relation::Friendly;
            let started = times.start(
                ActionCheck {
                    player: None,
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
                places,
            };
            let admission = books.admits(&holdings, ask);
            if researched || admission == TrainAdmission::Refused {
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
            times.commit(
                SlotCommit {
                    slot: ordered.slot,
                    now,
                    ready_at: now.after(values.cooldown),
                    charges: values.charges,
                    toggle: None,
                    player: None,
                },
                None,
            );
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
                if let Some(&spec) = trains.get(head.action) {
                    // A train at a place makes its unit at a free one of its producer, and waits
                    // while each holds a unit (04-capabilities/production.md, A jet holds a place).
                    let places = world.get::<HeldPlaces>(entity).filter(|_| spec.at_place);
                    let place = match places.map(HeldPlaces::free) {
                        None => None,
                        Some(Some(place)) => Some(place),
                        Some(None) => {
                            // Due again at once, it is looked at each tick until a place frees.
                            let queue = world.get::<TrainQueue>(entity).expect("a producer");
                            let again = queue.head_done().expect("a queue of a done head");
                            Self::due_again(world, trainer, entity, again);
                            break;
                        }
                    };
                    // A train at a place leaves by its place's door; another leaves a producer of
                    // places by none, as Zero Hour's helipad needs no door, and any other producer
                    // by its first (04-capabilities/production.md, A producer's door opens before
                    // its unit leaves).
                    let door = match place {
                        Some(place) => Some(place),
                        None => (!world.entity(entity).contains::<HeldPlaces>()).then_some(0),
                    };
                    if let Some(open_at) =
                        door.and_then(|door| Self::through_door(world, entity, door, now))
                    {
                        Self::due_again(world, trainer, entity, open_at);
                        break;
                    }
                    let unit = match place {
                        None => Self::leave(world, entity, spec.unit_type),
                        Some(place) => Self::leave_at(world, entity, spec.unit_type, place),
                    };
                    world.resource_mut::<TrainedUnits>().0.extend([Trained {
                        unit,
                        producer: trainer,
                    }]);
                } else {
                    Self::give(world, entity, trainer, head.action);
                }
                world.resource_scope(|world, mut index: Mut<'_, DueIndex<TrainQueue>>| {
                    let mut queue = world.get_mut::<TrainQueue>(entity).expect("a producer");
                    let dues = &mut UnitDues::new(&mut index, trainer, entity);
                    queue.remove(0, now, dues);
                });
            }
        }
    }

    /// Pushes the queue of the producer `trainer` of `entity` due again at `at`.
    fn due_again(world: &mut World, trainer: StableId, entity: Entity, at: Tick) {
        world.resource_scope(|_, mut index: Mut<'_, DueIndex<TrainQueue>>| {
            UnitDues::new(&mut index, trainer, entity).push(Some(at));
        });
    }

    /// Lets a unit of the producer of `entity` through the door of its place `place` at `now`, as
    /// `Doors::pass` does; none for a producer of no doors or once it left, else the tick it may
    /// leave.
    fn through_door(world: &mut World, entity: Entity, place: u8, now: Tick) -> Option<Tick> {
        let times = Self::door_times(world, entity)?;
        world.get_mut::<Doors>(entity)?.pass(place, now, times)
    }

    /// The door times of the producer of `entity`, none for one of no doors.
    fn door_times(world: &World, entity: Entity) -> Option<DoorTimes> {
        let producer_type = *world
            .get::<UnitType>(entity)
            .expect("a producer has a type");
        world
            .resource::<ById<UnitType, ProducerSpec>>()
            .get(producer_type)?
            .doors
    }

    /// Gives what the research `action` of the producer `producer` of `entity` gives: its modifier,
    /// to the producer's player, or to the producer from itself, with no end
    /// (04-capabilities/production.md, Research is an action kind of production). A player research
    /// of a producer that lost its player since it started gives nothing, as no player holds it.
    fn give(world: &mut World, entity: Entity, producer: StableId, action: ActionId) {
        let researches = world.resource::<ById<ActionId, ResearchSpec>>();
        let spec = *researches
            .get(action)
            .expect("a queue holds trains and researches");
        match spec.holder {
            ResearchHolder::Player => {
                let Some(owner) = world.get::<Owner>(entity).map(|owner| owner.player_slot())
                else {
                    return;
                };
                world
                    .player_modifiers_mut(owner)
                    .expect("a player of the session holds its modifiers")
                    .add(spec.modifier);
            }
            ResearchHolder::Producer => {
                Stats::apply_unending(world, producer, spec.modifier, producer);
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

    /// Spawns a unit of `unit_type` for the free place `place` of the producer of `entity`, at the
    /// place's exit or, of none, at the place, facing as the place turns with the producer, of its
    /// team and player, through the mode's spawner, and holds the place for it; no rally sends it
    /// on. Its id.
    fn leave_at(world: &mut World, entity: Entity, unit_type: UnitType, place: u8) -> StableId {
        let producer = world.entity(entity);
        let position = producer.get::<Position>().expect("a producer stands").get();
        let angle = producer
            .get::<Facing>()
            .map_or(Num::ZERO, |facing| facing.degrees());
        let id = *producer.get::<StableId>().expect("a producer has an id");
        let producer_type = *producer.get::<UnitType>().expect("a producer has a type");
        let spec = world
            .resource::<ById<UnitType, ProducerSpec>>()
            .get(producer_type);
        let data = spec
            .expect("a producer of places has its production")
            .places[usize::from(place)];
        let point = data.exit.unwrap_or(data.at);
        let mut at = Self::spawn_at(world, entity, unit_type, point.placed(position, angle));
        at.angle = angle + data.angle;
        let made = at.id;
        Spawner::spawn(
            world,
            at,
            MadeAt {
                producer: id,
                place,
            },
        );
        let mut places = world
            .get_mut::<HeldPlaces>(entity)
            .expect("a producer of places");
        places.hold(place, made);
        let now = world.resource::<SimTick>().end();
        if let Some(times) = Self::door_times(world, entity)
            && let Some(mut doors) = world.get_mut::<Doors>(entity)
        {
            doors.hold(place, now, times);
        }
        made
    }

    /// Frees the place each unit made at one held, as it dies, for its producer's next train
    /// (04-capabilities/production.md, A jet holds a place).
    pub(super) fn free_places(
        (tick, index): (Res<'_, SimTick>, Res<'_, EntityIndex>),
        dead: Query<'_, '_, (&StableId, &MadeAt), Added<Dead>>,
        mut producers: Query<'_, '_, (&mut HeldPlaces, Option<&mut Doors>)>,
    ) {
        for (&unit, &MadeAt { producer, place }) in &dead {
            let held = index
                .get(producer)
                .and_then(|producer| producers.get_mut(producer).ok());
            if let Some((mut held, doors)) = held
                && held.release(place, unit)
                && let Some(mut doors) = doors
            {
                doors.release(place, tick.end(), held.held());
            }
        }
    }

    /// Where a unit of `unit_type` leaves the producer of `entity`: of its team and player, at
    /// its spawn point, with a new id, and the point it rallies to.
    pub(crate) fn exit_spawn(world: &mut World, entity: Entity, unit_type: UnitType) -> ExitSpawn {
        let points = TrainPoints::of(world, entity);
        let out_to = points.drives_out(world, entity);
        let position = match (out_to, points.exit) {
            (Some(_), Some(exit)) => exit,
            _ => points.spawn_point(world, entity, unit_type),
        };
        ExitSpawn {
            at: Self::spawn_at(world, entity, unit_type, position),
            out_to,
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
            out_to: None,
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

    /// Sends `unit`, which left the producer of `entity` by `exit`, on: a gatherer to the node the
    /// producer rallies to, by its first gather of the node's resource, else, of a producer that
    /// starts its gatherers, to the free node its first gather finds, either to be checked as the
    /// next tick's orders are; any other to the producer's rally point. A unit that walks leaves,
    /// passing through its team's units until it clears them (04-capabilities/navigation.md, A new
    /// unit leaves its exit).
    pub(crate) fn send_on(world: &mut World, entity: Entity, unit: Entity, exit: ExitSpawn) {
        if let Some(mut leaving) = world.get_mut::<Leaving>(unit) {
            Leaving::set(&mut leaving, true);
        }
        // A unit made inside its producer drives out through it at its step, and then walks
        // its route (04-capabilities/production.md, A trained unit drives out of its producer).
        let step = world.get::<MoveStep>(unit).map(|step| step.get());
        if let Some((to, step)) = exit.out_to.zip(step.filter(|&step| step > Num::ZERO)) {
            let through = *world.get::<StableId>(entity).expect("a producer has an id");
            world
                .entity_mut(unit)
                .insert(ForcedMove::Exit { to, step, through });
        }
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
        let natural = point_of(data.and_then(|data| data.rally));
        Self {
            exit: point_of(data.and_then(|data| data.exit)),
            rally: Trains::rally_point(world, entity).or(natural),
            natural,
        }
    }

    /// The default rally point a unit made at the exit point drives out to, when the exit point
    /// lies within the body of the producer of `entity`, as Zero Hour makes a unit inside its
    /// factory and sends it along its exit path to its `NaturalRallyPoint`
    /// (04-capabilities/production.md, A trained unit drives out of its producer); none for an exit
    /// point outside it, or a producer of no default rally point.
    fn drives_out(self, world: &World, entity: Entity) -> Option<Vec3> {
        let (exit, natural) = (self.exit?, self.natural?);
        let producer = world.entity(entity);
        let body = producer.get::<Body>()?;
        let position = producer.get::<Position>().expect("a producer stands").get();
        let inside =
            body.shape().nearest_point(position, exit) == Vec3::new(exit.x, position.y, exit.z);
        inside.then_some(natural)
    }

    /// Where a unit of `unit_type` the producer of `entity` trains spawns: at the center of the
    /// cell nearest the exit point, or with none the point of the producer's body nearest the
    /// rally point, or with neither the producer's position, among the cells its walker may stand
    /// in, as a teleport finds its cell, so it stands outside the producer's body and every other
    /// (04-capabilities/navigation.md, A spawn stands where it can walk); with no such cell, or a
    /// unit that does not walk, at the exit point, or the producer's position. A producer with
    /// neither point and no body spawns at its position. A box's nearest point rounds to the
    /// nearest bit; a circle's is a step of its radius toward the rally point, as a walker steps.
    fn spawn_point(self, world: &mut World, entity: Entity, unit_type: UnitType) -> Vec3 {
        let producer = world.entity(entity);
        let position = producer.get::<Position>().expect("a producer stands").get();
        let near = match (self.exit, self.rally) {
            (Some(exit), _) => exit,
            (None, Some(to)) => Body::shape_of(producer.get::<Body>()).nearest_point(position, to),
            (None, None) if producer.contains::<Body>() => position,
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

/// What a research's refusal reads: the players' modifiers, the researches, the unit types'
/// upgrades, none in a match without stats, and the producers' queues.
#[derive(Debug)]
pub(crate) struct ResearchCheck<'a, 'w, 's, F: QueryFilter> {
    pub(crate) players: &'a PlayerRows<'w, 's, PlayerModifiers>,
    pub(crate) researches: &'a ById<ActionId, ResearchSpec>,
    pub(crate) upgrades: Option<&'a ById<UnitType, TypeUpgrades>>,
    pub(crate) queues: &'a Query<'w, 's, Queues, F>,
}

impl<F: QueryFilter> ResearchCheck<'_, '_, '_, F> {
    /// Whether the research `spec`, ordered of the unit `id` of `entity`, is refused: its modifier,
    /// or one its modifier withholds by its producer's type's upgrades, is held or queued already,
    /// a player research's by its producer's player or in the queue of any unit of that player, a
    /// producer research's by its producer, from itself, or in its own queue
    /// (04-capabilities/production.md, An upgrade is a modifier); or a modifier that withholds an
    /// upgrade its modifier triggers is held by the player or the producer, or queued as a player
    /// research by the player's units or as any research by the producer, as the upgrade's own test
    /// reads the player's and the unit's modifiers alike, so no research pays for an upgrade its
    /// conflict then withholds (riders-and-upgrades.md, A conflicting upgrade is refused). A player
    /// research of a producer of no player counts as held, as no player could hold it.
    pub(crate) fn refuses(&self, spec: ResearchSpec, id: StableId, entity: Entity) -> bool {
        let (_, &unit_type, owner, queue, carried) =
            self.queues.get(entity).expect("a producer has a queue");
        if spec.holder == ResearchHolder::Player && owner.is_none() {
            return true;
        }
        let upgrades = self.upgrades.and_then(|upgrades| upgrades.get(unit_type));
        let player = self.players.of(owner);
        let player_holds = |modifier| player.is_some_and(|held| held.holds(modifier));
        let own_holds =
            |modifier| carried.is_some_and(|carried| carried.get(modifier, Some(id)).is_some());
        // Whether `queue` holds a research of `modifier`, of `holder` or of either.
        let queued = |queue: &TrainQueue, holder: Option<ResearchHolder>, modifier| {
            queue.entries().iter().any(|queued| {
                self.researches.get(queued.action).is_some_and(|queued| {
                    queued.modifier == modifier
                        && holder.is_none_or(|holder| queued.holder == holder)
                })
            })
        };
        let player_queued = |holder: Option<ResearchHolder>, modifier| {
            owner.is_some_and(|owner| {
                self.queues.iter().any(|(_, _, unit_owner, queue, _)| {
                    unit_owner
                        .is_some_and(|unit_owner| unit_owner.player_slot() == owner.player_slot())
                        && queued(queue, holder, modifier)
                })
            })
        };
        let withheld = upgrades
            .into_iter()
            .flat_map(|upgrades| upgrades.withheld(spec.modifier));
        let mut own = iter::once(spec.modifier).chain(withheld);
        let held = own.any(|modifier| match spec.holder {
            ResearchHolder::Producer => {
                own_holds(modifier) || queued(queue, Some(spec.holder), modifier)
            }
            ResearchHolder::Player => {
                player_holds(modifier) || player_queued(Some(spec.holder), modifier)
            }
        });
        let mut withholding = upgrades
            .into_iter()
            .flat_map(|upgrades| upgrades.withholding(spec.modifier));
        held || withholding.any(|conflict| {
            player_holds(conflict)
                || own_holds(conflict)
                || queued(queue, None, conflict)
                || player_queued(Some(ResearchHolder::Player), conflict)
        })
    }
}
