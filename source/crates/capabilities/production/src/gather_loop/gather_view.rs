use std::slice;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Allow, Changed, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_actions::{ActionBook, ActionSlots, Aim, Caster};
use campfire_common::{PlayerSlot, Tick, Ticks};
use campfire_declarations::ById;
use campfire_geometry::Placed;
use campfire_math::{Num, Vec3};
use campfire_players::{OnMatch, PlayerResources, PlayerRows, ResourceAmount};
use campfire_script::Filter;
use campfire_sim::{EntityIndex, Position, SimTick, StableId, Unpredicted};
use campfire_units::{Body, Dead, Facing, MapBook, Owner, WalkReadOnlyItem};
use campfire_values::{ActionId, Motion, Relations, ResourceId, Team, UnitTags, UnitType};

use crate::gather_loop::WorkerParts;
use crate::gather_loop::checked_gather::CheckedGather;
use crate::gather_loop::gather::Gather;
use crate::gather_loop::location::LoopTarget;
use crate::gather_loop::step::Step;
use crate::gather_spec::GatherSpec;
use crate::gatherer::{GatherOrder, GatherStep, Gatherer, Load, NodeAt};
use crate::node::Node;
use crate::node_book::NodeBook;
use crate::site::Site;

/// What the gather loop reads of the match: the books, the workers, the nodes and the drop-offs.
/// A client reads the nodes it holds as the server sent them, as it predicts none.
#[derive(SystemParam, Debug)]
pub struct GatherView<'w, 's> {
    pub(super) tick: Res<'w, SimTick>,
    pub(super) index: Res<'w, EntityIndex>,
    pub(super) book: Res<'w, ActionBook>,
    pub(super) gathers: Res<'w, ById<ActionId, GatherSpec>>,
    pub(super) nodes_book: Res<'w, NodeBook>,
    pub(super) map: Res<'w, MapBook>,
    pub(super) relations: OnMatch<'w, 's, Relations>,
    pub(super) resources: PlayerRows<'w, 's, PlayerResources>,
    pub(super) workers: Query<'w, 's, WorkerParts, Without<Dead>>,
    pub(super) nodes: Query<
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
        (Without<Dead>, Allow<Unpredicted>),
    >,
    pub(super) drop_offs: Query<
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
    pub(super) changed: Query<'w, 's, Entity, (Changed<Gatherer>, Without<Dead>)>,
    pub(super) facings: Query<'w, 's, &'static Facing, Allow<Unpredicted>>,
}

/// A node's parts as the loop reads them.
type NodeRow<'a> = (
    Entity,
    &'a StableId,
    &'a Position,
    Option<&'a Body>,
    &'a UnitType,
    &'a Node,
    &'a Team,
    Option<&'a UnitTags>,
);

/// A worker as the loop looks at nodes for it: its gather, its team, and the match's relations,
/// read once for each step.
#[derive(Debug, Clone, Copy)]
struct Seeker<'a> {
    gather: Gather,
    team: Team,
    relations: &'a Relations,
}

/// What a worker's gather takes of a node: its resource and its filter, by how the worker's team
/// regards the node's.
#[derive(Debug, Clone, Copy)]
struct NodeRule<'a> {
    resource: ResourceId,
    filter: Filter,
    team: Team,
    relations: &'a Relations,
}

impl<'a> Seeker<'a> {
    /// The rule of the nodes its gather takes.
    const fn rule(self) -> NodeRule<'a> {
        NodeRule {
            resource: self.gather.spec.resource,
            filter: self.gather.filter,
            team: self.team,
            relations: self.relations,
        }
    }
}

/// A worker as it takes its load to a drop-off: where it stands, its owner, its loop, its load,
/// its node while it stands, how it looks at nodes, and its walk.
#[derive(Debug, Clone, Copy)]
struct Hauler<'a> {
    worker: LoopTarget,
    owner: Option<Owner>,
    order: GatherOrder,
    load: Option<Load>,
    node: Option<FoundNode>,
    seeker: Seeker<'a>,
    walk: Option<&'a WalkReadOnlyItem<'a, 'a>>,
}

/// A load a worker of `owner` brings: its resource, which a drop-off of the player takes or not.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Haul {
    pub(crate) owner: PlayerSlot,
    pub(crate) resource: ResourceId,
}

/// A node the loop found: its entity, and where it stands.
#[derive(Debug, Clone, Copy)]
pub(super) struct FoundNode {
    pub(super) entity: Entity,
    pub(super) location: LoopTarget,
}

impl GatherView<'_, '_> {
    /// The gather in `slot` of `slots`; `None` for a slot that holds none, or holds one not yet
    /// learned.
    pub(crate) fn gather(&self, slots: &ActionSlots, slot: u8) -> Option<Gather> {
        let action_slot = slots.slot(slot)?;
        let action = self.book.get(action_slot.action?)?;
        let spec = *self.gathers.get(action_slot.action?)?;
        let Aim::Unit(filter) = action.aim else {
            return None;
        };
        let rank = action_slot.rank?;
        let range = slots.meters_in(&self.book, slot)?;
        let ticks = action.values(rank).windup.get().max(1);
        Some(Gather {
            spec,
            filter,
            range,
            time: ticks,
        })
    }

    /// Whether the living worker `worker` gathers the living node `node` with the gather in
    /// `slot`, learned or not, as its loop takes a node: of the gather's resource, with something
    /// left, and one the gather's filter selects; whether it may gather then is its order's and its
    /// loop's own test (04-capabilities/control.md, A bot's search runs its order's own test).
    pub fn gathers(&self, worker: StableId, slot: u8, node: StableId) -> bool {
        let found = self
            .index
            .get(worker)
            .and_then(|at| self.workers.get(at).ok());
        let Some((.., &team, _, slots, _, _, _)) = found else {
            return false;
        };
        let Some(action) = slots.slot(slot).and_then(|held| held.action) else {
            return false;
        };
        let (Some(spec), Some(Aim::Unit(filter))) = (
            self.gathers.get(action),
            self.book.get(action).map(|action| action.aim),
        ) else {
            return false;
        };
        let relations = self.relations.get();
        let rule = NodeRule {
            resource: spec.resource,
            filter,
            team,
            relations: &relations,
        };
        let row = self.index.get(node).and_then(|at| self.nodes.get(at).ok());
        row.is_some_and(|row| self.takes_node(&row, rule))
    }

    /// The living node of id `node` that `seeker`'s gather gathers for it, with something left:
    /// one that ran out is gone to the loop, though it stands until the tick ends.
    fn node(&self, node: StableId, seeker: Seeker<'_>) -> Option<FoundNode> {
        self.found(self.nodes.get(self.index.get(node)?).ok()?, seeker)
    }

    /// Whether `rule`'s gather takes the node of `row`: one of its resource, with something left,
    /// that its filter selects.
    fn takes_node(&self, row: &NodeRow<'_>, rule: NodeRule<'_>) -> bool {
        let (_, _, _, _, &unit_type, node, &node_team, tags) = *row;
        let relation = rule.relations.between(rule.team, node_team);
        let tags = tags.map_or_default(|tags| tags.tags);
        node.amount() > 0
            && self.nodes_book.resource(unit_type) == Some(rule.resource)
            && rule.filter.selects(relation, tags)
    }

    /// The node of `row`, when `seeker`'s gather gathers it for it, as `node` finds one.
    fn found(&self, row: NodeRow<'_>, seeker: Seeker<'_>) -> Option<FoundNode> {
        let ours = self.takes_node(&row, seeker.rule());
        let (entity, _, &at, body, &unit_type, ..) = row;
        ours.then(|| FoundNode {
            entity,
            location: self.location(entity, unit_type, at.get(), body),
        })
    }

    /// Where a worker goes to at the unit of `entity`, of `unit_type`, at `at` of `body`: its
    /// dock's point, turned with it, for a type of one, else its body
    /// (04-capabilities/production.md, A dock has its point).
    fn location(
        &self,
        entity: Entity,
        unit_type: UnitType,
        at: Vec3,
        body: Option<&Body>,
    ) -> LoopTarget {
        match self.nodes_book.dock(unit_type) {
            Some(dock) => {
                let facing = self.facings.get(entity).ok();
                let angle = facing.map_or(Num::ZERO, |facing| facing.degrees());
                LoopTarget {
                    position: dock.placed(at, angle),
                    body: None,
                }
            }
            None => LoopTarget {
                position: at,
                body: body.copied(),
            },
        }
    }

    /// The nearest living node to a worker at `from` that `seeker`'s gather gathers, whose body
    /// comes within its bounce of `near`'s, and that no worker holds: by the distance to its
    /// body, the lower stable id on a tie.
    fn free_node_near(&self, from: Vec3, seeker: Seeker<'_>, near: LoopTarget) -> Option<NodeAt> {
        let near_shape = Body::shape_of(near.body.as_ref());
        self.nodes
            .iter()
            .filter(|(.., node, _, _)| node.holder().is_none())
            .filter_map(|row| {
                let id = *row.1;
                let location = self.found(row, seeker)?.location;
                let shape = Body::shape_of(location.body.as_ref());
                let within = self.map.metric().reaches(
                    Placed::new(near.position, near_shape),
                    seeker.gather.spec.bounce,
                    Placed::new(location.position, shape),
                );
                within.then_some((location.distance_from(from), id, location.position))
            })
            .min_by_key(|&(distance, id, _)| (distance, id))
            .map(|(_, node, at)| NodeAt { node, position: at })
    }

    /// Whether a drop-off of `unit_type` that `drop_owner` owns takes `delivery`: one of the
    /// worker's player that takes its resource.
    pub(crate) fn takes(&self, drop_owner: Owner, unit_type: UnitType, delivery: Haul) -> bool {
        drop_owner.player_slot() == delivery.owner
            && self.nodes_book.takes(unit_type, delivery.resource)
    }

    /// Where the living, complete drop-off `id` stands, when it takes `delivery`.
    fn drop_off_location(&self, id: StableId, delivery: Haul) -> Option<LoopTarget> {
        let entity = self.index.get(id)?;
        let (_, &at, body, &unit_type, drop_owner) = self.drop_offs.get(entity).ok()?;
        self.takes(*drop_owner, unit_type, delivery)
            .then(|| self.location(entity, unit_type, at.get(), body))
    }

    /// The nearest living, complete drop-off that takes `delivery`, to a worker at `from`: by
    /// the distance to its body, the lower stable id on a tie.
    pub(super) fn drop_off(&self, from: Vec3, delivery: Haul) -> Option<StableId> {
        self.drop_offs
            .iter()
            .filter(|&(.., &unit_type, drop_owner)| self.takes(*drop_owner, unit_type, delivery))
            .map(|(&id, &at, body, ..)| {
                let location = LoopTarget {
                    position: at.get(),
                    body: body.copied(),
                };
                (location.distance_from(from), id)
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
            .of_player_slot(owner)
            .is_some_and(|resources| resources.takes(slice::from_ref(&amount)))
    }

    /// Whether `worker`, with a gather of `gather`, reaches `at`.
    pub(super) fn reaches(&self, worker: LoopTarget, gather: Gather, at: LoopTarget) -> bool {
        let shape = Body::shape_of(worker.body.as_ref());
        let to = Body::shape_of(at.body.as_ref());
        self.map.metric().reaches(
            Placed::new(worker.position, shape),
            gather.range,
            Placed::new(at.position, to),
        )
    }

    /// What the worker of `entity` does in its loop this tick, with the match's `relations`, which
    /// the caller reads once for each read of the view.
    pub(crate) fn step(&self, entity: Entity, relations: &Relations) -> Step {
        let Ok((_, &id, &from, body, &team, owner, slots, gatherer, walk, tags)) =
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
        let seeker = Seeker {
            gather,
            team,
            relations,
        };
        let worker = LoopTarget {
            position: from.get(),
            body: body.copied(),
        };
        let node = self.node(order.node.node, seeker);
        if Caster::of(tags, Motion::Own, None).blocked(slots.slot(order.slot), &self.book) {
            return match (order.step, node) {
                (GatherStep::Gathering { .. }, Some(node)) => Step::Interrupt(node.entity),
                _ => Step::Hold,
            };
        }
        let gone = LoopTarget {
            position: order.node.position,
            body: None,
        };
        let search = || match self.free_node_near(from.get(), seeker, gone) {
            Some(next) => Step::Retarget(next),
            None => Step::Stop,
        };
        let hauler = Hauler {
            worker,
            owner: owner.copied(),
            order,
            load: gatherer.load(),
            node,
            seeker,
            walk: walk.as_ref(),
        };
        match order.step {
            GatherStep::Ordered => Step::Hold,
            GatherStep::ToNode => {
                let Some(FoundNode { entity, location }) = node else {
                    return search();
                };
                if !self.reaches(worker, gather, location) {
                    return location.walk_from(from.get(), walk.as_ref());
                }
                let (.., node, _, _) = self.nodes.get(entity).expect("a node found this tick");
                if node.holder().is_none() {
                    return Step::Take(entity);
                }
                match self.free_node_near(from.get(), seeker, location) {
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
                // No two workers gather one node at once: one another worker holds is not this
                // one's. A predicting client holds the node's holder as the server sent it.
                let (.., node, _, _) = self.nodes.get(entity).expect("a node found this tick");
                if node.holder().is_some_and(|holder| holder != id) {
                    return search();
                }
                if self.tick.start() >= since.after(Ticks::new(gather.time)) {
                    Step::Collect(entity)
                } else {
                    Step::Hold
                }
            }
            GatherStep::ToDropOff { drop_off } => self.haul(&hauler, drop_off, None),
            GatherStep::Delivering { drop_off, since } => {
                self.haul(&hauler, Some(drop_off), Some(since))
            }
        }
    }

    /// What a worker `at` its drop-off does with its load, its loop at `drop_off`, and, as it
    /// stands there, from the tick `since`: it walks to the drop-off, else stands there for its
    /// gather's time, then its load joins its player's, and it goes back to its node, or the free
    /// node nearest it within its bounce of the node's place once it is gone.
    fn haul(&self, at: &Hauler<'_>, drop_off: Option<StableId>, since: Option<Tick>) -> Step {
        let order = at.order;
        let Some(load) = at.load else {
            return Step::Deliver(at.node.map(|_| order.node));
        };
        // A worker no player owns has no drop-off.
        let Some(owner) = at.owner.map(Owner::player_slot) else {
            return Step::Stand;
        };
        let delivery = Haul {
            owner,
            resource: load.resource,
        };
        let from = at.worker.position;
        let chosen = drop_off.and_then(|id| Some((id, self.drop_off_location(id, delivery)?)));
        let Some((drop_off, location)) = chosen else {
            return match self.drop_off(from, delivery) {
                Some(next) => Step::Choose(next),
                None => Step::Stand,
            };
        };
        let gather = at.seeker.gather;
        if !self.reaches(at.worker, gather, location) {
            // One pushed off as it stood there walks back, and stands its whole time again.
            return match since {
                Some(_) => Step::Choose(drop_off),
                None => location.walk_from(from, at.walk),
            };
        }
        let deliver = gather.spec.deliver;
        match since {
            None if deliver != Ticks::ZERO => return Step::Unload(drop_off),
            Some(since) if self.tick.start() < since.after(deliver) => return Step::Hold,
            _ => {}
        }
        if !self.joins(owner, load) {
            return Step::Stand;
        }
        let gone = LoopTarget {
            position: order.node.position,
            body: None,
        };
        let back = match at.node {
            Some(_) => Some(order.node),
            None => self.free_node_near(from, at.seeker, gone),
        };
        Step::Deliver(back)
    }

    /// The gather loop of the worker of `entity` as it leaves a producer that starts its
    /// gatherers, with the match's `relations`: by the first gather in its slots, to the nearest
    /// free node it gathers whose body comes within the gather's bounce of the worker's, to be
    /// checked as the next tick's orders are; none with no such node.
    pub(crate) fn seek(&self, entity: Entity, relations: &Relations) -> Option<GatherOrder> {
        let (_, _, &from, body, &team, _, slots, ..) = self.workers.get(entity).ok()?;
        let (slot, gather) = slots
            .indexed()
            .find_map(|(slot, _)| Some((slot, self.gather(slots, slot)?)))?;
        let seeker = Seeker {
            gather,
            team,
            relations,
        };
        let here = LoopTarget {
            position: from.get(),
            body: body.copied(),
        };
        let node = self.free_node_near(from.get(), seeker, here)?;
        Some(GatherOrder {
            slot,
            node,
            step: GatherStep::Ordered,
        })
    }

    /// The check of the gather order of the worker of `entity`, when it applied this tick, with
    /// the match's `relations`.
    pub(super) fn resolve(&self, entity: Entity, relations: &Relations) -> Option<CheckedGather> {
        let (_, _, _, _, &team, owner, slots, gatherer, ..) = self.workers.get(entity).ok()?;
        let order = gatherer.order()?;
        if order.step != GatherStep::Ordered {
            return None;
        }
        let checked = |order, load| {
            Some(CheckedGather {
                entity,
                order,
                load,
            })
        };
        let Some(gather) = self.gather(slots, order.slot) else {
            return checked(None, gatherer.load());
        };
        let target = order.node.node;
        let seeker = Seeker {
            gather,
            team,
            relations,
        };
        if let Some(found) = self.node(target, seeker) {
            let node = NodeAt {
                node: target,
                position: found.location.position,
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
        let returns = gatherer.load().zip(owner).is_some_and(|(load, owner)| {
            let delivery = Haul {
                owner: owner.player_slot(),
                resource: load.resource,
            };
            let location = self.drop_off_location(target, delivery);
            location.is_some()
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

/// A node a worker holds, as the loop starts.
#[derive(Debug, Clone, Copy)]
pub(crate) struct HeldNode {
    pub(super) entity: Entity,
    pub id: StableId,
    pub(super) holder: StableId,
}
