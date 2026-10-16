use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Added, Changed, Or, QueryState, Without};
use bevy_ecs::system::{Commands, Local, ParamSet, Query, SystemState};
use bevy_ecs::world::{Mut, World};
use campfire_actions::{ActionBook, ActionSlots, KindSpec};
use campfire_declare::RoleForm;
use campfire_navigation::Navigation;
use campfire_players::PlayersWorld;
use campfire_sim::{
    EntityIndex, Keyed, Ordered, Position, Removals, SimTick, StableId, Unpredicted,
};
use campfire_units::{ArrivedShort, Body, Dead, Destination, Owner, StatusTags};
use campfire_values::{Team, UnitTags};

use crate::Production;
use crate::gather_loop::checked_gather::CheckedGather;
use crate::gather_loop::gather_view::{GatherView, HeldNode};
use crate::gather_loop::step::Step;
use crate::gatherer::{GatherOrder, GatherStep, Gatherer, Load, NodeAt};
use crate::node::Node;

pub(crate) mod checked_gather;
pub(crate) mod gather;
pub(crate) mod gather_view;
pub(crate) mod place;
pub(crate) mod step;

/// The systems of the gather loop: an order checked as it applies, the loop's steps, the nodes
/// that ran out, and the `gathering` tag.
#[derive(Debug)]
pub struct GatherLoop;

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
    Option<&'static ArrivedShort>,
    Option<&'static UnitTags>,
);

/// What tells the loop that a worker may have left the node it held since its last run: the
/// workers whose loop another system changed, and those that went.
pub(super) type LeftWorkers<'w, 's> = (
    Query<'w, 's, (), Changed<Gatherer>>,
    Removals<'w, 's, Gatherer>,
);

/// The living units that gather, which the loop visits and a freed node's waiting workers are
/// found among.
pub(super) type Gatherers =
    QueryState<(Entity, &'static StableId, &'static Gatherer), Without<Dead>>;

/// A loop a worker left: the worker's entity, and its order in the loop.
#[derive(Debug, Clone, Copy)]
struct LeftLoop {
    worker: Entity,
    order: GatherOrder,
}

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
            if view.changed.is_empty() {
                return;
            }
            let relations = view.relations.get();
            for entity in &view.changed {
                if let Some(next) = view.resolve(entity, &relations) {
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

    /// Ends the gather order of each worker that died since the last run, as each capability ends
    /// its order of a unit at its death: a dead unit takes no order, and holds no node after its
    /// respawn.
    pub(crate) fn forget_dead_gathers(mut workers: Query<'_, '_, &mut Gatherer, Added<Dead>>) {
        for mut worker in &mut workers {
            if worker.order().is_some() {
                worker.set(None);
            }
        }
    }

    /// Runs each worker's loop, in Act, after the trains and the builds start, by stable id, as
    /// `GatherView::step` says: a step that chooses, gathers a load or delivers one is followed
    /// in the same tick by the next, until the worker walks, stands, waits, gathers or stops.
    /// First, when another system changed a worker's loop or a worker went since the last run,
    /// each node whose holder no longer gathers it, as its loop ended or it died, frees; a
    /// worker whose own step leaves the node it gathers frees it as it leaves. A node frees to
    /// its first waiting worker, by the tick it began to wait, then by stable id, whose gather's
    /// time runs from this tick.
    pub(crate) fn run<F: RoleForm>(
        world: &mut World,
        (workers, nodes): (&mut Gatherers, &mut QueryState<(Entity, &StableId, &Node)>),
        (view, left): (
            &mut SystemState<GatherView<'_, '_>>,
            &mut SystemState<LeftWorkers<'_, '_>>,
        ),
        (mut order, mut held_nodes): (Local<'_, Ordered>, Local<'_, Vec<HeldNode>>),
    ) {
        let (changed, mut gone) = left
            .get_mut(world)
            .expect("the left workers are always valid");
        let went = gone.read().count() != 0;
        if went || !changed.is_empty() {
            Self::free_left_nodes(world, workers, nodes, &mut held_nodes);
        }
        let looping = workers
            .iter(world)
            .filter(|(.., gatherer)| gatherer.order().is_some())
            .map(|(entity, &id, _)| Keyed { id, entity });
        let mut due = order.sort(looping).iter();
        if due.len() == 0 {
            return;
        }
        // A hold changes nothing, so one read of the view serves every worker that holds, up
        // to the next that changes the world.
        loop {
            let next = {
                let read = view.get(world).expect("a gather view is always valid");
                let relations = read.relations.get();
                due.by_ref()
                    .map(|&Keyed { entity, .. }| (entity, read.step(entity, &relations)))
                    .find(|&(_, step)| !matches!(step, Step::Hold))
            };
            let Some((entity, mut step)) = next else {
                break;
            };
            for taken in 1..=Self::STEPS {
                Self::apply::<F>(world, workers, entity, step);
                if !step.goes_on() || taken == Self::STEPS {
                    break;
                }
                let read = view.get(world).expect("a gather view is always valid");
                step = read.step(entity, &read.relations.get());
            }
        }
    }

    /// Frees each node whose holder no longer gathers it: dead, gone, or of a loop that ended or
    /// went elsewhere. A client frees none, as it holds every node as the server sent it, whose
    /// holder is the server's word.
    fn free_left_nodes(
        world: &mut World,
        workers: &mut Gatherers,
        nodes: &mut QueryState<(Entity, &StableId, &Node)>,
        held_nodes: &mut Vec<HeldNode>,
    ) {
        held_nodes.clear();
        held_nodes.extend(nodes.iter(world).filter_map(|(entity, &id, node)| {
            Some(HeldNode {
                entity,
                id,
                holder: node.holder()?,
            })
        }));
        held_nodes.sort_unstable_by_key(|node| node.id);
        for &HeldNode {
            entity: node,
            id,
            holder,
        } in &*held_nodes
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
                Self::free(world, workers, node, id);
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
        if let Some(mut node) = Self::node_mut(world, entity) {
            node.hold(first.map(|(_, worker_id, _)| worker_id));
        }
        if let Some((_, _, worker)) = first {
            let mut gatherer = world.get_mut::<Gatherer>(worker).expect("a waiting worker");
            let order = gatherer.order().expect("a waiting worker's loop");
            gatherer.set(Some(GatherOrder {
                step: GatherStep::Gathering { since: now },
                ..order
            }));
        }
    }

    /// Frees the node its worker held in the loop it `left`, when it gathered there and the node
    /// stands and holds it.
    fn leave(world: &mut World, workers: &mut Gatherers, left: LeftLoop) {
        let LeftLoop {
            worker: entity,
            order,
        } = left;
        if !matches!(order.step, GatherStep::Gathering { .. }) {
            return;
        }
        let id = order.node.node;
        let Some(node) = world.resource::<EntityIndex>().get(id) else {
            return;
        };
        let worker = world.get::<StableId>(entity).copied();
        let holder = world.get::<Node>(node).and_then(|node| node.holder());
        if holder.is_some() && holder == worker {
            Self::free(world, workers, node, id);
        }
    }

    /// Does `step` for the worker of `entity`.
    fn apply<F: RoleForm>(world: &mut World, workers: &mut Gatherers, entity: Entity, step: Step) {
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
            Step::Walk(to) => Navigation::go_to(world, entity, Some(to)),
            Step::Stand => Navigation::go_to(world, entity, None),
            Step::Stop => {
                Navigation::go_to(world, entity, None);
                set(world, None);
                let left = LeftLoop {
                    worker: entity,
                    order,
                };
                Self::leave(world, workers, left);
            }
            Step::Interrupt(node) => {
                set(world, with(GatherStep::ToNode));
                let id = *world.get::<StableId>(node).expect("a node has an id");
                Self::free(world, workers, node, id);
            }
            Step::Retarget(next) => {
                set(
                    world,
                    Some(GatherOrder {
                        node: next,
                        step: GatherStep::ToNode,
                        ..order
                    }),
                );
                let left = LeftLoop {
                    worker: entity,
                    order,
                };
                Self::leave(world, workers, left);
            }
            Step::Wait => {
                Navigation::go_to(world, entity, None);
                set(world, with(GatherStep::Waiting { since: now }));
            }
            Step::Take(node) => {
                Navigation::go_to(world, entity, None);
                let id = *world.get::<StableId>(entity).expect("a worker has an id");
                if let Some(mut node) = Self::node_mut(world, node) {
                    node.hold(Some(id));
                }
                set(world, with(GatherStep::Gathering { since: now }));
            }
            Step::Collect(node) => Self::collect(world, workers, entity, order, node),
            Step::Choose(drop_off) => set(
                world,
                with(GatherStep::ToDropOff {
                    drop_off: Some(drop_off),
                }),
            ),
            Step::Deliver(back) => Self::deliver::<F>(world, entity, order, back),
        }
    }

    /// Ends the gather of the worker of `entity`, in its loop `order`, at the node of `node`: it
    /// carries what it takes, sets out for a drop-off, and frees the node. A client takes nothing
    /// from a node it holds as the server sent it: its worker carries what the take would take.
    fn collect(
        world: &mut World,
        workers: &mut Gatherers,
        entity: Entity,
        order: GatherOrder,
        node: Entity,
    ) {
        let slots = world.get::<ActionSlots>(entity).expect("a worker's slots");
        let action_id = slots.slot(order.slot).and_then(|slot| slot.action);
        let action = action_id.and_then(|action| world.resource::<ActionBook>().get(action));
        let Some(&KindSpec::Gather(spec)) = action.map(|action| &action.kind) else {
            panic!("a worker's loop is of its gather");
        };
        let taken = match Self::node_mut(world, node) {
            Some(mut held) => held.take(spec.take),
            None => world
                .get::<Node>(node)
                .expect("a node gathered")
                .taken(spec.take),
        };
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
        Self::free(world, workers, node, id);
    }

    /// The node of `entity` to write; none on a client for a node it holds as the server sent
    /// it, whose amount and holder are the server's word, as a client writes nothing of one
    /// (14-replication-audiences.md, A client derives what it can for every unit it holds).
    fn node_mut(world: &mut World, entity: Entity) -> Option<Mut<'_, Node>> {
        if world.entity(entity).contains::<Unpredicted>() {
            return None;
        }
        Some(world.get_mut::<Node>(entity).expect("a node"))
    }

    /// Joins the load of the worker of `entity`, which its player's resources take, to them, and
    /// sends it `back` to a node, or ends its loop with none. A predicting client's form `F` joins
    /// nothing to the resources, which are the server's, and drops the load as the server does.
    fn deliver<F: RoleForm>(
        world: &mut World,
        entity: Entity,
        order: GatherOrder,
        back: Option<NodeAt>,
    ) {
        Navigation::go_to(world, entity, None);
        let gatherer = *world.get::<Gatherer>(entity).expect("a worker");
        if let Some(load) = gatherer.load().filter(|_| !F::PREDICTS) {
            let owner = *world
                .get::<Owner>(entity)
                .expect("a load joins its owner's");
            world
                .player_resources_mut(owner.slot())
                .expect("a player of the session holds its resources")
                .add(load.resource, i64::from(load.amount))
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

    /// Despawns each node that ran out, as the tick ends, of those that changed since the last
    /// run; its waiting workers look for another as the loop next runs.
    pub(crate) fn despawn_empty_nodes(
        nodes: Query<'_, '_, (Entity, &Node), (Changed<Node>, Without<Dead>)>,
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
            (&Gatherer, &mut StatusTags),
            Or<(Changed<Gatherer>, Changed<StatusTags>)>,
        >,
    ) {
        for (gatherer, mut status) in &mut workers {
            let looping = gatherer
                .order()
                .is_some_and(|order| order.step != GatherStep::Ordered);
            let wanted = status.turned(Production::GATHERING, looping);
            status.set_if_neq(wanted);
        }
    }
}
