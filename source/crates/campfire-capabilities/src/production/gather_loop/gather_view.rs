use std::slice;

use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Without};
use bevy_ecs::system::{Query, Res, SystemParam};
use campfire_common::{PlayerSlot, Ticks};
use campfire_sim::{EntityIndex, Position, SimTick, StableId};

use crate::actions::action::Aim;
use crate::actions::action_book::ActionBook;
use crate::actions::action_slots::ActionSlots;
use crate::actions::kind_spec::KindSpec;
use crate::geometry::metric::Metric;
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::players::resource_id::ResourceId;
use crate::production::gather_loop::Worker;
use crate::production::gather_loop::checked_gather::CheckedGather;
use crate::production::gather_loop::gather::Gather;
use crate::production::gather_loop::place::Place;
use crate::production::gather_loop::step::Step;
use crate::production::gatherer::{GatherOrder, GatherStep, Gatherer, Load, NodeAt};
use crate::production::node::Node;
use crate::production::node_book::NodeBook;
use crate::production::site::Site;
use crate::units::block::Block;
use crate::units::body::Body;
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::relations::Relations;
use crate::units::team::Team;
use crate::units::unit_tags::UnitTags;
use crate::units::unit_type::UnitType;

/// What the gather loop reads of the match: the books, the workers, the nodes and the drop-offs.
#[derive(SystemParam, Debug)]
pub(crate) struct GatherView<'w, 's> {
    pub(super) tick: Res<'w, SimTick>,
    pub(super) index: Res<'w, EntityIndex>,
    pub(super) book: Res<'w, ActionBook>,
    pub(super) nodes_book: Res<'w, NodeBook>,
    pub(super) metric: Res<'w, Metric>,
    pub(super) relations: Res<'w, Relations>,
    pub(super) resources: Option<Res<'w, PlayerResources>>,
    pub(super) workers: Query<'w, 's, Worker, Without<Dead>>,
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
        Without<Dead>,
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
}

/// A node the loop found: its entity, and where it stands.
#[derive(Debug, Clone, Copy)]
pub(super) struct FoundNode {
    pub(super) entity: Entity,
    pub(super) place: Place,
}

impl GatherView<'_, '_> {
    /// The gather in `slot` of `slots`; `None` for a slot that holds none.
    pub(super) fn gather(&self, slots: &ActionSlots, slot: u8) -> Option<Gather> {
        let held = slots.slot(slot)?;
        let action = self.book.get(held.action?)?;
        let (KindSpec::Gather(spec), Aim::Unit(filter)) = (action.kind, action.aim) else {
            return None;
        };
        let range = self.book.meters(slots, slot);
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
    pub(super) fn node(&self, node: StableId, gather: Gather, team: Team) -> Option<FoundNode> {
        let (entity, _, &at, body, &unit_type, held, &node_team, tags) =
            self.nodes.get(self.index.get(node)?).ok()?;
        let relation = self.relations.between(team, node_team);
        let tags = tags.map(|tags| tags.tags).unwrap_or_default();
        let ours = held.amount() > 0
            && self.nodes_book.resource(unit_type) == Some(gather.spec.resource)
            && gather.filter.selects(relation, tags);
        ours.then_some(FoundNode {
            entity,
            place: Place {
                at,
                body: body.copied(),
            },
        })
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
                within.then_some((place.distance_from(from), id, place.at))
            })
            .min_by_key(|&(distance, id, _)| (distance, id))
            .map(|(_, node, at)| NodeAt { node, at })
    }

    /// Whether a drop-off of `unit_type` that `drop_owner` owns takes `resource` from a worker
    /// of `owner`: one of the same player that takes that resource.
    pub(super) fn takes(
        &self,
        drop_owner: Owner,
        unit_type: UnitType,
        owner: PlayerSlot,
        resource: ResourceId,
    ) -> bool {
        drop_owner.slot() == owner && self.nodes_book.takes(unit_type, resource)
    }

    /// Where the living, complete drop-off `id` stands, when it takes `resource` from a worker
    /// of `owner`.
    fn drop_off_place(
        &self,
        id: StableId,
        owner: PlayerSlot,
        resource: ResourceId,
    ) -> Option<Place> {
        let (_, &at, body, &unit_type, drop_owner) =
            self.drop_offs.get(self.index.get(id)?).ok()?;
        self.takes(*drop_owner, unit_type, owner, resource)
            .then_some(Place {
                at,
                body: body.copied(),
            })
    }

    /// The nearest living, complete drop-off of `owner` that takes `resource`, to a worker at
    /// `from`: by the distance to its body, the lower stable id on a tie.
    pub(super) fn drop_off(
        &self,
        from: Position,
        owner: PlayerSlot,
        resource: ResourceId,
    ) -> Option<StableId> {
        self.drop_offs
            .iter()
            .filter(|&(.., &unit_type, drop_owner)| {
                self.takes(*drop_owner, unit_type, owner, resource)
            })
            .map(|(&id, &at, body, ..)| {
                let place = Place {
                    at,
                    body: body.copied(),
                };
                (place.distance_from(from), id)
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
    pub(super) fn reaches(
        &self,
        from: Position,
        body: Option<&Body>,
        gather: Gather,
        place: Place,
    ) -> bool {
        let shape = Body::shape_of(body);
        let to = Body::shape_of(place.body.as_ref());
        self.metric.reaches(from, shape, gather.range, place.at, to)
    }

    /// What the worker of `entity` does in its loop this tick.
    pub(super) fn step(&self, entity: Entity) -> Step {
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
                    return place.walk_from(from, destination, route);
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
                let chosen = drop_off.and_then(|id| self.drop_off_place(id, owner, load.resource));
                let Some(place) = chosen else {
                    return match self.drop_off(from, owner, load.resource) {
                        Some(next) => Step::Choose(next),
                        None => Step::Stand,
                    };
                };
                if !self.reaches(from, body, gather, place) {
                    return place.walk_from(from, destination, route);
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

    /// The check of the gather order of the worker of `entity`, when it applied this tick.
    pub(super) fn resolve(&self, entity: Entity) -> Option<CheckedGather> {
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
        let returns = gatherer.load().zip(owner).is_some_and(|(load, owner)| {
            let place = self.drop_off_place(target, owner.slot(), load.resource);
            place.is_some()
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
    pub(super) id: StableId,
    pub(super) holder: StableId,
}
