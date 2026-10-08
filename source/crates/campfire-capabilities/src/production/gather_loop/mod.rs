use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Or, QueryState, Without};

use bevy_ecs::system::{Commands, Local, ParamSet, Query, SystemState};
use bevy_ecs::world::World;
use campfire_sim::{EntityIndex, Keyed, Ordered, Position, SimTick, StableId};

use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::kind_spec::KindSpec;
use crate::navigation::destination::Destination;
use crate::navigation::route::Route;
use crate::players::player_resources::PlayerResources;
use crate::production::gather_loop::checked_gather::CheckedGather;
use crate::production::gather_loop::gather_view::{GatherView, HeldNode};
use crate::production::gather_loop::step::Step;
use crate::production::gatherer::{GatherOrder, GatherStep, Gatherer, Load, NodeAt};
use crate::production::node::Node;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::engine_tag::EngineTag;
use crate::units::owner::Owner;
use crate::units::status_tags::StatusTags;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;

pub(crate) mod checked_gather;
pub(crate) mod gather;
pub(crate) mod gather_view;
pub(crate) mod place;
pub(crate) mod step;

/// The systems of the gather loop: an order checked as it applies, the loop's steps, the nodes
/// that ran out, and the `gathering` tag.
#[derive(Debug)]
pub(crate) struct GatherLoop;

/// The parts of a worker the loop reads.
pub(super) type Worker = (
    Entity,
    &'static StableId,
    &'static Position,
    Option<&'static Body>,
    &'static Team,
    Option<&'static Owner>,
    &'static ActionSlots,
    &'static Gatherer,
    Option<&'static Destination>,
    Option<&'static Route>,
    Option<&'static UnitTags>,
);

/// The living units that gather, which the loop visits and a freed node's waiting workers are
/// found among.
pub(super) type Gatherers =
    QueryState<(Entity, &'static StableId, &'static Gatherer), Without<Dead>>;

impl GatherLoop {
    /// The most steps a worker's loop takes in a tick: a gather that ends, a drop-off chosen, a
    /// load delivered and a node chosen, before it walks.
    const STEPS: usize = 5;

    /// Checks each gather order that applied since the last run, in Inputs, after the orders
    /// apply, and each a rally gave a trained unit: one aimed at a node its gather gathers goes
    /// to it, first to a drop-off with a load of its resource, and drops a load of another; one
    /// aimed at a drop-off of its player that takes its load returns the load, then goes back to
    /// the node it gathered last; any other ends.
    pub(crate) fn check_gathers(
        mut parts: ParamSet<'_, '_, (GatherView<'_, '_>, Query<'_, '_, &mut Gatherer>)>,
        mut checked: Local<'_, Vec<CheckedGather>>,
    ) {
        checked.clear();
        {
            let view = parts.p0();
            for entity in &view.changed {
                if let Some(next) = view.resolve(entity) {
                    checked.push(next);
                }
            }
        }
        let mut gatherers = parts.p1();
        for &CheckedGather {
            entity,
            order,
            load,
        } in &*checked
        {
            let mut gatherer = gatherers
                .get_mut(entity)
                .expect("a gatherer read this tick");
            gatherer.set(order);
            gatherer.carry(load);
        }
    }

    /// Runs each worker's loop, in Act, after the trains and the builds start, by stable id, as
    /// `GatherView::step` says: a step that chooses, gathers a load or delivers one is followed
    /// in the same tick by the next, until the worker walks, stands, waits, gathers or stops.
    /// First each node whose holder no longer gathers it, as its loop ended or it died, frees. A
    /// node frees to its first waiting worker, by the tick it began to wait, then by stable id,
    /// whose gather's time runs from this tick.
    pub(crate) fn run(
        world: &mut World,
        (workers, nodes): (&mut Gatherers, &mut QueryState<(Entity, &StableId, &Node)>),
        view: &mut SystemState<GatherView<'_, '_>>,
        (mut order, mut held): (Local<'_, Ordered>, Local<'_, Vec<HeldNode>>),
    ) {
        GatherLoop::free_left_nodes(world, workers, nodes, &mut held);
        let looping = workers
            .iter(world)
            .filter(|(.., gatherer)| gatherer.order().is_some())
            .map(|(entity, &id, _)| Keyed { id, entity });
        let mut due = order.sort(looping).iter();
        // A hold changes nothing, so one read of the view serves every worker that holds, up
        // to the next that changes the world.
        loop {
            let next = {
                let read = view.get(world).expect("a gather view is always valid");
                due.by_ref()
                    .map(|&Keyed { entity, .. }| (entity, read.step(entity)))
                    .find(|&(_, step)| !matches!(step, Step::Hold))
            };
            let Some((entity, mut step)) = next else {
                break;
            };
            for taken in 1..=GatherLoop::STEPS {
                GatherLoop::apply(world, workers, entity, step);
                if !step.goes_on() || taken == GatherLoop::STEPS {
                    break;
                }
                step = view
                    .get(world)
                    .expect("a gather view is always valid")
                    .step(entity);
            }
        }
    }

    /// Frees each node whose holder no longer gathers it: dead, gone, or of a loop that ended or
    /// went elsewhere.
    fn free_left_nodes(
        world: &mut World,
        workers: &mut Gatherers,
        nodes: &mut QueryState<(Entity, &StableId, &Node)>,
        held: &mut Vec<HeldNode>,
    ) {
        held.clear();
        held.extend(nodes.iter(world).filter_map(|(entity, &id, node)| {
            Some(HeldNode {
                entity,
                id,
                holder: node.holder()?,
            })
        }));
        held.sort_unstable_by_key(|node| node.id);
        for &HeldNode {
            entity: node,
            id,
            holder,
        } in &*held
        {
            let index = world.resource::<EntityIndex>();
            let gathers = index.get(holder).is_some_and(|unit| {
                let unit = world.entity(unit);
                !unit.contains::<Dead>()
                    && unit
                        .get::<Gatherer>()
                        .and_then(|gatherer| gatherer.order())
                        .is_some_and(|order| {
                            order.node.node == id
                                && matches!(order.step, GatherStep::Gathering { .. })
                        })
            });
            if !gathers {
                GatherLoop::free(world, workers, node, id);
            }
        }
    }

    /// Frees the node of `entity`, of id `id`, to its first waiting worker, by the tick it began
    /// to wait, then by stable id: it holds the node, and its gather's time runs from this tick.
    fn free(world: &mut World, workers: &mut Gatherers, entity: Entity, id: StableId) {
        let now = world.resource::<SimTick>().start();
        let first = workers
            .iter(world)
            .filter_map(|(worker, &worker_id, gatherer)| {
                let order = gatherer.order()?;
                let GatherStep::Waiting { since } = order.step else {
                    return None;
                };
                (order.node.node == id).then_some((since, worker_id, worker))
            })
            .min_by_key(|&(since, worker_id, _)| (since, worker_id));
        let mut node = world.get_mut::<Node>(entity).expect("a node to free");
        node.hold(first.map(|(_, worker_id, _)| worker_id));
        if let Some((_, _, worker)) = first {
            let mut gatherer = world.get_mut::<Gatherer>(worker).expect("a waiting worker");
            let order = gatherer.order().expect("a waiting worker's loop");
            gatherer.set(Some(GatherOrder {
                step: GatherStep::Gathering { since: now },
                ..order
            }));
        }
    }

    /// Does `step` for the worker of `entity`.
    fn apply(world: &mut World, workers: &mut Gatherers, entity: Entity, step: Step) {
        let now = world.resource::<SimTick>().start();
        let Some(order) = world
            .get::<Gatherer>(entity)
            .and_then(|gatherer| gatherer.order())
        else {
            return;
        };
        let set = |world: &mut World, next: Option<GatherOrder>| {
            let mut gatherer = world.get_mut::<Gatherer>(entity).expect("a worker");
            gatherer.set(next);
        };
        let with = |step| Some(GatherOrder { step, ..order });
        match step {
            Step::Hold => {}
            Step::Walk(to) => Destination::go(world, entity, Some(to)),
            Step::Stand => Destination::go(world, entity, None),
            Step::Stop => {
                Destination::go(world, entity, None);
                set(world, None);
            }
            Step::Interrupt(node) => {
                set(world, with(GatherStep::ToNode));
                let id = *world.get::<StableId>(node).expect("a node has an id");
                GatherLoop::free(world, workers, node, id);
            }
            Step::Retarget(next) => set(
                world,
                Some(GatherOrder {
                    node: next,
                    step: GatherStep::ToNode,
                    ..order
                }),
            ),
            Step::Wait => {
                Destination::go(world, entity, None);
                set(world, with(GatherStep::Waiting { since: now }));
            }
            Step::Take(node) => {
                Destination::go(world, entity, None);
                let id = *world.get::<StableId>(entity).expect("a worker has an id");
                world
                    .get_mut::<Node>(node)
                    .expect("a node to take")
                    .hold(Some(id));
                set(world, with(GatherStep::Gathering { since: now }));
            }
            Step::Collect(node) => GatherLoop::collect(world, workers, entity, order, node),
            Step::Choose(drop_off) => set(
                world,
                with(GatherStep::ToDropOff {
                    drop_off: Some(drop_off),
                }),
            ),
            Step::Deliver(back) => GatherLoop::deliver(world, entity, order, back),
        }
    }

    /// Ends the gather of the worker of `entity`, in its loop `order`, at the node of `node`: it
    /// carries what it takes, sets out for a drop-off, and frees the node.
    fn collect(
        world: &mut World,
        workers: &mut Gatherers,
        entity: Entity,
        order: GatherOrder,
        node: Entity,
    ) {
        let slots = world.get::<ActionSlots>(entity).expect("a worker's slots");
        let held = slots.slot(order.slot).and_then(|slot| slot.action);
        let action = held.and_then(|action| world.resource::<ActionBook>().get(action));
        let Some(KindSpec::Gather(spec)) = action.map(|action| action.kind) else {
            panic!("a worker's loop is of its gather");
        };
        let taken = world
            .get_mut::<Node>(node)
            .expect("a node gathered")
            .take(spec.take);
        let mut gatherer = world.get_mut::<Gatherer>(entity).expect("a worker");
        gatherer.carry(Some(Load {
            resource: spec.resource,
            amount: taken,
        }));
        gatherer.set(Some(GatherOrder {
            step: GatherStep::ToDropOff { drop_off: None },
            ..order
        }));
        let id = *world.get::<StableId>(node).expect("a node has an id");
        GatherLoop::free(world, workers, node, id);
    }

    /// Joins the load of the worker of `entity`, which its player's resources take, to them, and
    /// sends it `back` to a node, or ends its loop with none.
    fn deliver(world: &mut World, entity: Entity, order: GatherOrder, back: Option<NodeAt>) {
        Destination::go(world, entity, None);
        let gatherer = *world.get::<Gatherer>(entity).expect("a worker");
        if let Some(load) = gatherer.load() {
            let owner = world
                .get::<Owner>(entity)
                .expect("a load joins its owner's");
            let owner = owner.slot();
            world
                .resource_mut::<PlayerResources>()
                .add(owner, load.resource, i64::from(load.amount))
                .expect("the step checked the player's resources take the load");
        }
        let mut gatherer = world.get_mut::<Gatherer>(entity).expect("a worker");
        gatherer.carry(None);
        gatherer.set(back.map(|node| GatherOrder {
            node,
            step: GatherStep::ToNode,
            ..order
        }));
    }

    /// Despawns each node that ran out, as the tick ends; its waiting workers look for another
    /// as the loop next runs.
    pub(crate) fn despawn_empty_nodes(
        nodes: Query<'_, '_, (Entity, &Node), Without<Dead>>,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, node) in &nodes {
            if node.amount() == 0 {
                commands.entity(entity).despawn();
            }
        }
    }

    /// Gives each worker in its loop the engine tag `gathering`, and takes it from one whose loop
    /// ended. The tag follows from the worker's loop alone, so a run visits only the workers whose
    /// loop or status tags changed since its last.
    pub(crate) fn tag_gatherers(
        mut workers: Query<
            '_,
            '_,
            (Entity, &Gatherer, Option<&mut StatusTags>),
            Or<(Changed<Gatherer>, Changed<StatusTags>)>,
        >,
        mut commands: Commands<'_, '_>,
    ) {
        for (entity, gatherer, status) in &mut workers {
            let looping = gatherer
                .order()
                .is_some_and(|order| order.step != GatherStep::Ordered);
            match status {
                Some(mut status) => {
                    let wanted = status.turned(EngineTag::Gathering, looping);
                    status.set_if_neq(wanted);
                }
                None if looping => {
                    commands
                        .entity(entity)
                        .insert(StatusTags::of([EngineTag::Gathering]));
                }
                None => {}
            }
        }
    }
}
