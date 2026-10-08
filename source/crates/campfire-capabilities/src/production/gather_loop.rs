use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Or, QueryState, Without};
use std::slice;

use bevy_ecs::system::{Commands, Local, ParamSet, Query, Res, SystemParam, SystemState};
use bevy_ecs::world::World;
use campfire_common::{PlayerSlot, Ticks};
use campfire_math::Num;
use campfire_sim::{EntityIndex, Keyed, Ordered, Position, SimTick, StableId};

use crate::actions::action::Aim;
use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::gather_spec::GatherSpec;
use crate::actions::kind_spec::KindSpec;
use crate::actions::range::Range;
use crate::navigation::destination::Destination;
use crate::navigation::route::Route;
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::production::gatherer::{GatherOrder, GatherStep, Gatherer, Load, NodeAt};
use crate::production::node::Node;
use crate::production::node_book::NodeBook;
use crate::production::site::Site;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::engine_tag::EngineTag;
use crate::units::filter::Filter;
use crate::units::owner::Owner;
use crate::units::relations::Relations;
use crate::units::status_tags::StatusTags;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;
use crate::values::metric::Metric;

/// The systems of the gather loop: an order checked as it applies, the loop's steps, the nodes
/// that ran out, and the `gathering` tag.
#[derive(Debug)]
pub(crate) struct GatherLoop;

/// The parts of a worker the loop reads.
type Worker = (
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
type Gatherers = QueryState<(Entity, &'static StableId, &'static Gatherer), Without<Dead>>;

/// What the gather loop reads of the match: the books, the workers, the nodes and the drop-offs.
#[derive(SystemParam, Debug)]
pub(crate) struct GatherView<'w, 's> {
    tick: Res<'w, SimTick>,
    index: Res<'w, EntityIndex>,
    book: Res<'w, ActionBook>,
    nodes_book: Res<'w, NodeBook>,
    metric: Res<'w, Metric>,
    relations: Res<'w, Relations>,
    resources: Option<Res<'w, PlayerResources>>,
    workers: Query<'w, 's, Worker, Without<Dead>>,
    nodes: Query<
        'w,
        's,
        (
            Entity,
            &'static StableId,
            &'static Position,
            Option<&'static Body>,
            &'static UnitType,
            &'static Node,
            &'static Team,
            Option<&'static UnitTags>,
        ),
        Without<Dead>,
    >,
    drop_offs: Query<
        'w,
        's,
        (
            &'static StableId,
            &'static Position,
            Option<&'static Body>,
            &'static UnitType,
            &'static Owner,
        ),
        (Without<Dead>, Without<Site>),
    >,
    changed: Query<'w, 's, Entity, (Changed<Gatherer>, Without<Dead>)>,
}

/// A worker's gather: its action's spec, its filter of nodes, its range, and its time in ticks.
#[derive(Debug, Clone, Copy)]
struct Gather {
    spec: GatherSpec,
    filter: Filter,
    range: Num,
    time: u64,
}

/// A unit the loop goes to: where it stands, and its body.
#[derive(Debug, Clone, Copy)]
struct Place {
    at: Position,
    body: Option<Body>,
}

/// A node the loop found: its entity, and where it stands.
#[derive(Debug, Clone, Copy)]
struct FoundNode {
    entity: Entity,
    place: Place,
}

/// What a worker does in its loop this tick.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// Nothing changes.
    Hold,
    /// It walks to the point.
    Walk(Position),
    /// It stands where it is, its step as it was.
    Stand,
    /// Its loop ends.
    Stop,
    /// Its gather ends with no load, as a tag blocks its `use` group, and its node frees.
    Interrupt(Entity),
    /// It goes on to this node.
    Retarget(NodeAt),
    /// It waits at its node.
    Wait,
    /// It holds the node of this entity, and gathers.
    Take(Entity),
    /// Its gather of the node of this entity ends: it takes the most a trip carries.
    Collect(Entity),
    /// It takes its load to this drop-off.
    Choose(StableId),
    /// Its load joins its player's resources: then on to the node, or none to stop.
    Deliver(Option<NodeAt>),
}

impl Step {
    /// Whether the loop takes another step in the tick after this one.
    const fn goes_on(self) -> bool {
        matches!(
            self,
            Step::Retarget(_) | Step::Collect(_) | Step::Choose(_) | Step::Deliver(_)
        )
    }
}

impl GatherView<'_, '_> {
    /// The gather in `slot` of `slots`; `None` for a slot that holds none.
    fn gather(&self, slots: &ActionSlots, slot: u8) -> Option<Gather> {
        let held = slots.slot(slot)?;
        let action = self.book.get(held.action?)?;
        let (KindSpec::Gather(spec), Aim::Unit(filter)) = (action.kind, action.aim) else {
            return None;
        };
        let Range::Meters(range) = self.book.range(slots, slot) else {
            panic!("the load checked a gather's range in meters");
        };
        let rank = held.rank.expect("a gather's slot is learned");
        let ticks = action.values(rank).windup.get().max(1);
        Some(Gather {
            spec,
            filter,
            range,
            time: ticks,
        })
    }

    /// The living node of id `node` that `gather` gathers for a worker of `team`, with something
    /// left: one that ran out is gone to the loop, though it stands until the tick ends.
    fn node(&self, node: StableId, gather: Gather, team: Team) -> Option<FoundNode> {
        let (entity, _, &at, body, &unit_type, held, &node_team, tags) =
            self.nodes.get(self.index.get(node)?).ok()?;
        let attitude = self.relations.between(team, node_team);
        let tags = tags.map(|tags| tags.tags).unwrap_or_default();
        let ours = held.amount() > 0
            && self.nodes_book.resource(unit_type) == Some(gather.spec.resource)
            && gather.filter.selects(attitude, tags);
        ours.then_some(FoundNode {
            entity,
            place: Place {
                at,
                body: body.copied(),
            },
        })
    }

    /// The squared distance on the ground plane from `from` to the point of `place`'s body
    /// nearest it, as `Shape::nearest_point` rounds it, in squared bits.
    fn distance(from: Position, place: Place) -> u128 {
        let near = Body::shape_of(place.body.as_ref()).nearest_point(place.at, from);
        from.ground_offset(near).length_squared_bits()
    }

    /// The nearest living node to a worker at `from` of `team` that its `gather` gathers, whose
    /// body comes within its bounce of `near`'s, and that no worker holds: by the distance to its
    /// body, the lower stable id on a tie.
    fn free_node_near(
        &self,
        from: Position,
        team: Team,
        gather: Gather,
        near: Place,
    ) -> Option<NodeAt> {
        let near_shape = Body::shape_of(near.body.as_ref());
        self.nodes
            .iter()
            .filter(|(.., node, _, _)| node.holder().is_none())
            .filter_map(|(_, &id, ..)| {
                let place = self.node(id, gather, team)?.place;
                let shape = Body::shape_of(place.body.as_ref());
                let within =
                    self.metric
                        .reaches(near.at, near_shape, gather.spec.bounce, place.at, shape);
                within.then_some((GatherView::distance(from, place), id, place.at))
            })
            .min_by_key(|&(distance, id, _)| (distance, id))
            .map(|(_, node, at)| NodeAt { node, at })
    }

    /// The nearest living, complete drop-off of `owner` that takes `resource`, to a worker at
    /// `from`: by the distance to its body, the lower stable id on a tie.
    fn drop_off(
        &self,
        from: Position,
        owner: PlayerSlot,
        resource: ResourceId,
    ) -> Option<StableId> {
        self.drop_offs
            .iter()
            .filter(|&(.., &unit_type, drop_owner)| {
                drop_owner.slot() == owner && self.nodes_book.takes(unit_type, resource)
            })
            .map(|(&id, &at, body, ..)| {
                let place = Place {
                    at,
                    body: body.copied(),
                };
                (GatherView::distance(from, place), id)
            })
            .min()
            .map(|(_, id)| id)
    }

    /// Whether player `owner`'s resources take `load`: a match that keeps them, and an amount the
    /// load does not carry past the largest an amount holds.
    fn joins(&self, owner: PlayerSlot, load: Load) -> bool {
        let amount = ResourceAmount {
            resource: load.resource,
            amount: i64::from(load.amount),
        };
        self.resources
            .as_ref()
            .is_some_and(|resources| resources.takes(owner, slice::from_ref(&amount)))
    }

    /// Whether a worker at `from` with a body of `body` and a gather of `gather` reaches `place`.
    fn reaches(&self, from: Position, body: Option<&Body>, gather: Gather, place: Place) -> bool {
        let shape = Body::shape_of(body);
        let to = Body::shape_of(place.body.as_ref());
        self.metric.reaches(from, shape, gather.range, place.at, to)
    }

    /// Walks a worker at `from`, whose route and destination are given, to `place`'s point
    /// nearest it, or stops it when its route arrived short of that point.
    fn walk(
        from: Position,
        place: Place,
        destination: Option<&Destination>,
        route: Option<&Route>,
    ) -> Step {
        let to = Body::shape_of(place.body.as_ref()).nearest_point(place.at, from);
        if Destination::gives_up(destination, route, to) {
            Step::Stop
        } else {
            Step::Walk(to)
        }
    }

    /// What the worker of `entity` does in its loop this tick.
    fn step(&self, entity: Entity) -> Step {
        let Ok((_, _, &from, body, &team, owner, slots, gatherer, destination, route, tags)) =
            self.workers.get(entity)
        else {
            return Step::Hold;
        };
        let Some(order) = gatherer.order() else {
            return Step::Hold;
        };
        let Some(gather) = self.gather(slots, order.slot) else {
            return Step::Stop;
        };
        let node = self.node(order.node.node, gather, team);
        if UnitTags::properties_of(tags).blocks(Block::Use) {
            return match (order.step, node) {
                (GatherStep::Gathering { .. }, Some(node)) => Step::Interrupt(node.entity),
                _ => Step::Hold,
            };
        }
        let gone = Place {
            at: order.node.at,
            body: None,
        };
        let search = || match self.free_node_near(from, team, gather, gone) {
            Some(next) => Step::Retarget(next),
            None => Step::Stop,
        };
        match order.step {
            GatherStep::Ordered => Step::Hold,
            GatherStep::ToNode => {
                let Some(FoundNode { entity, place }) = node else {
                    return search();
                };
                if !self.reaches(from, body, gather, place) {
                    return GatherView::walk(from, place, destination, route);
                }
                let (.., node, _, _) = self.nodes.get(entity).expect("a node found this tick");
                if node.holder().is_none() {
                    return Step::Take(entity);
                }
                match self.free_node_near(from, team, gather, place) {
                    Some(next) => Step::Retarget(next),
                    None => Step::Wait,
                }
            }
            GatherStep::Waiting { .. } => match node {
                Some(_) => Step::Hold,
                None => search(),
            },
            GatherStep::Gathering { since } => {
                let Some(FoundNode { entity, .. }) = node else {
                    return search();
                };
                if self.tick.start() >= since.after(Ticks::new(gather.time)) {
                    Step::Collect(entity)
                } else {
                    Step::Hold
                }
            }
            GatherStep::ToDropOff { drop_off } => {
                let Some(load) = gatherer.load() else {
                    return Step::Deliver(node.map(|_| order.node));
                };
                // A worker no player owns has no drop-off.
                let Some(owner) = owner.copied().map(Owner::slot) else {
                    return Step::Stand;
                };
                let chosen = drop_off.and_then(|id| {
                    let (_, &at, body, &unit_type, drop_owner) =
                        self.drop_offs.get(self.index.get(id)?).ok()?;
                    let takes = drop_owner.slot() == owner
                        && self.nodes_book.takes(unit_type, load.resource);
                    takes.then_some(Place {
                        at,
                        body: body.copied(),
                    })
                });
                let Some(place) = chosen else {
                    return match self.drop_off(from, owner, load.resource) {
                        Some(next) => Step::Choose(next),
                        None => Step::Stand,
                    };
                };
                if !self.reaches(from, body, gather, place) {
                    return GatherView::walk(from, place, destination, route);
                }
                if !self.joins(owner, load) {
                    return Step::Stand;
                }
                let back = match node {
                    Some(_) => Some(order.node),
                    None => self.free_node_near(from, team, gather, gone),
                };
                Step::Deliver(back)
            }
        }
    }
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
        mut checked: Local<'_, Vec<Checked>>,
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
        for &Checked {
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

/// A node a worker holds, as the loop starts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HeldNode {
    entity: Entity,
    id: StableId,
    holder: StableId,
}

/// A gather order as its check leaves it: the worker, its loop, and its load.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Checked {
    entity: Entity,
    order: Option<GatherOrder>,
    load: Option<Load>,
}

impl GatherView<'_, '_> {
    /// The check of the gather order of the worker of `entity`, when it applied this tick.
    fn resolve(&self, entity: Entity) -> Option<Checked> {
        let (_, _, _, _, &team, owner, slots, gatherer, ..) = self.workers.get(entity).ok()?;
        let order = gatherer.order()?;
        if order.step != GatherStep::Ordered {
            return None;
        }
        let checked = |order, load| {
            Some(Checked {
                entity,
                order,
                load,
            })
        };
        let Some(gather) = self.gather(slots, order.slot) else {
            return checked(None, gatherer.load());
        };
        let target = order.node.node;
        if let Some(found) = self.node(target, gather, team) {
            let node = NodeAt {
                node: target,
                at: found.place.at,
            };
            let load = gatherer
                .load()
                .filter(|load| load.resource == gather.spec.resource);
            let step = match load {
                Some(_) => GatherStep::ToDropOff { drop_off: None },
                None => GatherStep::ToNode,
            };
            return checked(
                Some(GatherOrder {
                    node,
                    step,
                    ..order
                }),
                load,
            );
        }
        let drop_off = self.index.get(target);
        let returns =
            (gatherer.load().zip(owner).zip(drop_off)).is_some_and(|((load, owner), drop_off)| {
                self.drop_offs
                    .get(drop_off)
                    .is_ok_and(|(.., &unit_type, drop_owner)| {
                        drop_owner.slot() == owner.slot()
                            && self.nodes_book.takes(unit_type, load.resource)
                    })
            });
        if !returns {
            return checked(None, gatherer.load());
        }
        let node = gatherer
            .last()
            .expect("a load comes of a gather, which keeps its node");
        let step = GatherStep::ToDropOff {
            drop_off: Some(target),
        };
        checked(
            Some(GatherOrder {
                node,
                step,
                ..order
            }),
            gatherer.load(),
        )
    }
}
