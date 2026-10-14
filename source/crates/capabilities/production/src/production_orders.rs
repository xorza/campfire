use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Commands, Local, Query, Res};
use campfire_players::{PlayerResources, PlayerRowsMut, ResourceAmount, TickCommands};
use campfire_sim::{EntityIndex, SimTick};
use campfire_units::{Dead, MapBook, Owner};
use campfire_values::Team;
use campfire_vision::{SeenTargets, TeamSight};

use crate::build_book::BuildBook;
use crate::production_action::ProductionAction;
use crate::rally::Rally;
use crate::rally_target::RallyTarget;
use crate::site::Site;
use crate::train_queue::TrainQueue;

/// The tick's orders that cancel a train or a build, or set a rally.
#[derive(Debug)]
pub struct ProductionOrders;

impl ProductionOrders {
    /// Applies each production order of the tick, in input order, to each of its units by stable id
    /// that its player controls: a cancel of a train or a rally to a unit with a train queue, dead
    /// or not, a cancel of a build to a living site, as a dead one refunds nothing. A cancel of a
    /// train names an entry by its place in the queue as it stands; a place past its end is
    /// ignored. Its entry leaves the queue and its player gets back the player resources it paid; a
    /// head's leaving starts the next one's time in this tick. A cancel of a build gives back the
    /// build's `cancel_refund` of each player resource its build paid, each rounded down, and
    /// despawns the site, with no death. A refund that would carry an amount past an `i64` refuses
    /// its cancel. A rally sets the producer's rally point, a point taken into the bounds, or a
    /// unit its player's vision group sees, or clears it; a rally to another unit is ignored.
    pub(super) fn apply_production_orders<S: TeamSight>(
        (tick, map, index): (Res<'_, SimTick>, Res<'_, MapBook>, Res<'_, EntityIndex>),
        (mut orders, builds): (TickCommands<'_, '_, ProductionAction>, Res<'_, BuildBook>),
        mut resources: PlayerRowsMut<'_, '_, PlayerResources>,
        mut units: Query<
            '_,
            '_,
            (
                &Owner,
                Option<&Team>,
                Option<&mut TrainQueue>,
                Option<&Site>,
                Has<Dead>,
            ),
        >,
        visible: SeenTargets<'_, '_, S>,
        mut commands: Commands<'_, '_>,
        (mut refund, mut cancelled): (Local<'_, Vec<ResourceAmount>>, Local<'_, Vec<Entity>>),
    ) {
        let now = tick.start();
        cancelled.clear();
        let mut refunds = |amounts: &[ResourceAmount], slot| {
            amounts.is_empty()
                || resources
                    .of_slot_mut(slot)
                    .expect("an order that paid resources runs in a match with them")
                    .refund(amounts)
        };
        orders.for_each(|order| {
            for &unit in order.units {
                let Some(entity) = index.get(unit) else {
                    continue;
                };
                let Ok((owner, team, queue, site, dead)) = units.get_mut(entity) else {
                    continue;
                };
                if !order.controls(*owner) {
                    continue;
                }
                match (order.action, queue, site) {
                    (ProductionAction::CancelTrain { place }, Some(mut queue), _) => {
                        let place = usize::from(place);
                        let Some(paid) = queue.paid(place) else {
                            continue;
                        };
                        if refunds(paid, order.slot) {
                            queue.remove(place, now);
                        }
                    }
                    (
                        ProductionAction::Rally {
                            target: Some(RallyTarget::Unit(unit)),
                        },
                        Some(_),
                        _,
                    ) if !team.is_some_and(|&team| visible.sees(team, unit)) => {}
                    (ProductionAction::Rally { target }, Some(_), _) => {
                        let target = target.map(|target| match target {
                            RallyTarget::Point { x, z } => {
                                let [x, z] = map.bounds.clamp_ground([x, z]);
                                RallyTarget::Point { x, z }
                            }
                            unit @ RallyTarget::Unit(_) => unit,
                        });
                        match target {
                            Some(target) => commands.entity(entity).insert(Rally::new(target)),
                            None => commands.entity(entity).remove::<Rally>(),
                        };
                    }
                    (ProductionAction::CancelBuild, _, Some(site))
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
                        if refunds(&refund, order.slot) {
                            commands.entity(entity).despawn();
                            cancelled.push(entity);
                        }
                    }
                    _ => {}
                }
            }
        });
    }
}
