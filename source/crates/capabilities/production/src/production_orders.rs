use bevy_ecs::change_detection::DetectChangesMut;
use bevy_ecs::entity::Entity;
use bevy_ecs::observer::On;
use bevy_ecs::query::{Allow, Has};
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_players::{
    OrderedUnits, PlayerResources, PlayerRowsMut, ResourceAmount, TickCommands,
};
use campfire_sim::{DueIndex, SimTick, StableId, UnitDues, Unpredicted};
use campfire_units::{Dead, GroundPoint, MapBook, OrderKind, UnitOrdered};
use campfire_values::Team;
use campfire_vision::{ByVision, SeenTargets, TeamSight};

use crate::build_book::BuildBook;
use crate::builder::{BuildOrder, Builder};
use crate::gatherer::{GatherOrder, GatherStep, Gatherer, NodeAt};
use crate::production_directive::ProductionDirective;
use crate::rally::Rally;
use crate::rally_target::RallyTarget;
use crate::site::Site;
use crate::train_queue::{QueueIndex, TrainQueue};

/// The tick's orders that cancel a train or a build, or set a rally.
#[derive(Debug)]
pub struct ProductionOrders;

impl ProductionOrders {
    /// The observer that sets a unit's build order and gather loop as an order is given: a
    /// build's or a gather's sets its own and ends the other, and any other order ends both. A
    /// unit with neither keeps its parts untouched.
    pub(super) fn ordered(
        ordered: On<'_, '_, UnitOrdered>,
        mut units: Query<
            '_,
            '_,
            (&StableId, Option<&mut Builder>, Option<&mut Gatherer>),
            Allow<Unpredicted>,
        >,
        (mut builders, mut gatherers): (
            ResMut<'_, DueIndex<Builder>>,
            ResMut<'_, DueIndex<Gatherer>>,
        ),
    ) {
        let entity = ordered.entity;
        let Ok((&id, builder, gatherer)) = units.get_mut(entity) else {
            return;
        };
        let (build, gather) = match ordered.order {
            OrderKind::Build { slot, target } => (Some(BuildOrder { slot, target }), None),
            OrderKind::Gather { slot, node, from } => {
                let gather = GatherOrder {
                    slot,
                    node: NodeAt {
                        node,
                        position: from,
                    },
                    step: GatherStep::Ordered,
                };
                (None, Some(gather))
            }
            OrderKind::Use { .. } | OrderKind::Other => (None, None),
        };
        if let Some(mut builder) = builder
            && (build.is_some() || builder.order().is_some())
        {
            builder.set(build, &mut UnitDues::new(&mut builders, id, entity));
        }
        if let Some(mut gatherer) = gatherer
            && (gather.is_some() || gatherer.order().is_some())
        {
            gatherer.set(gather, &mut UnitDues::new(&mut gatherers, id, entity));
        }
    }

    /// Applies each cancel of the tick, in input order, to each of its units by stable id that
    /// its player controls: a cancel of a train to a unit with a train queue, dead or not, a
    /// cancel of a build to a living site, as a dead one refunds nothing. A cancel of a train
    /// names an entry by its place in the queue as it stands; a place past its end is ignored.
    /// Its entry leaves the queue and its player gets back the player resources it paid; a head's
    /// leaving starts the next one's time in this tick. A cancel of a build gives back the
    /// build's `cancel_refund` of each player resource its build paid, each rounded down, and
    /// despawns the site, with no death. A refund that would carry an amount past an `i64`
    /// refuses its cancel. In a predicting client's form `S`, a cancel refunds nothing and
    /// despawns no site: both are the server's, whose word comes back.
    pub(super) fn apply_cancels<S: TeamSight>(
        tick: Res<'_, SimTick>,
        (mut orders, builds): (
            TickCommands<'_, '_, ProductionDirective>,
            Res<'_, BuildBook>,
        ),
        mut resources: PlayerRowsMut<'_, '_, PlayerResources>,
        mut units: OrderedUnits<'_, '_, (Option<&mut TrainQueue>, Option<&Site>, Has<Dead>)>,
        mut commands: Commands<'_, '_>,
        (mut refund, mut cancelled): (Local<'_, Vec<ResourceAmount>>, Local<'_, Vec<Entity>>),
        mut queues: QueueIndex<'_>,
    ) {
        let now = tick.start();
        cancelled.clear();
        // A predicting client refunds nothing, as a player's resources are the server's
        // (10-replication-audiences.md, Prediction reads only what its client holds): its cancel
        // stands as the server's refund will.
        let mut refunds = |amounts: &[ResourceAmount], player_slot| {
            S::PREDICTS
                || amounts.is_empty()
                || resources
                    .of_player_slot_mut(player_slot)
                    .expect("an order that paid resources runs in a match with them")
                    .refund(amounts)
        };
        orders.for_each(|order| {
            units.for_each_mut(&order, |unit, entity, (queue, site, dead)| {
                match (order.directive, queue, site) {
                    (ProductionDirective::CancelTrain { place }, Some(mut queue), _) => {
                        let place = usize::from(place);
                        let Some(paid) = queue.paid(place) else {
                            return;
                        };
                        if refunds(paid, order.player_slot) {
                            queue.remove(place, now, &mut queues.of(unit, entity));
                        }
                    }
                    (ProductionDirective::CancelBuild, _, Some(site))
                        if !dead && !cancelled.contains(&entity) =>
                    {
                        let spec = builds
                            .of(site.action())
                            .expect("a site's build is in the book");
                        refund.clear();
                        refund.extend(site.paid().iter().map(|paid| ResourceAmount {
                            resource: paid.resource,
                            amount: spec.refund.of(paid.amount),
                        }));
                        // A site's despawn is the server's, as a client despawns no unit.
                        if refunds(&refund, order.player_slot) && !S::PREDICTS {
                            commands.entity(entity).despawn();
                            cancelled.push(entity);
                        }
                    }
                    _ => {}
                }
            });
        });
    }

    /// Applies each rally of the tick, in input order, to each of its units by stable id that
    /// its player controls and that produces, dead or not: it sets the producer's rally point, a
    /// point taken into the bounds, or a unit its player's vision group sees, or clears it; a
    /// rally to another unit is ignored. A rally is the server's, which no client holds.
    pub(super) fn set_rallies(
        map: Res<'_, MapBook>,
        mut orders: TickCommands<'_, '_, ProductionDirective>,
        mut producers: OrderedUnits<'_, '_, (Option<&Team>, &mut Rally)>,
        visible: SeenTargets<'_, '_, ByVision>,
    ) {
        orders.for_each(|order| {
            let ProductionDirective::Rally { target } = order.directive else {
                return;
            };
            let target = target.map(|target| match target {
                RallyTarget::Point(GroundPoint { x, z }) => {
                    let [x, z] = map.bounds.clamp_ground([x, z]);
                    RallyTarget::Point(GroundPoint { x, z })
                }
                unit @ RallyTarget::Unit(_) => unit,
            });
            producers.for_each_mut(&order, |_, _, (team, mut rally)| {
                let seen = match target {
                    Some(RallyTarget::Unit(unit)) => {
                        team.is_some_and(|&team| visible.sees(team, unit))
                    }
                    _ => true,
                };
                if seen {
                    rally.set_if_neq(Rally::new(target));
                }
            });
        });
    }
}
