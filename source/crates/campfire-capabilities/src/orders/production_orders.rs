use bevy_ecs::entity::Entity;
use bevy_ecs::query::Has;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_sim::{EntityIndex, SimTick};

use crate::geometry::bounds::Bounds;
use crate::orders::order::Action;
use crate::orders::tick_orders::TickOrders;
use crate::players::player_resources::PlayerResources;
use crate::players::resource_amount::ResourceAmount;
use crate::production::build_specs::BuildSpecs;
use crate::production::rally::Rally;
use crate::production::rally_target::RallyTarget;
use crate::production::site::Site;
use crate::production::train_queue::TrainQueue;
use crate::units::dead::Dead;
use crate::units::owner::Owner;

/// The tick's orders that cancel a train or a build, or set a rally.
#[derive(Debug)]
pub(super) struct ProductionOrders;

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
    /// unit, or clears it.
    pub(super) fn apply_production_orders(
        (tick, bounds, orders, index): (
            Res<'_, SimTick>,
            Res<'_, Bounds>,
            Res<'_, TickOrders>,
            Res<'_, EntityIndex>,
        ),
        builds: Res<'_, BuildSpecs>,
        mut resources: Option<ResMut<'_, PlayerResources>>,
        mut units: Query<'_, '_, (&Owner, Option<&mut TrainQueue>, Option<&Site>, Has<Dead>)>,
        mut commands: Commands<'_, '_>,
        (mut refund, mut cancelled): (Local<'_, Vec<ResourceAmount>>, Local<'_, Vec<Entity>>),
    ) {
        let now = tick.start();
        cancelled.clear();
        let mut refunds = |amounts: &[ResourceAmount], slot| {
            amounts.is_empty()
                || resources
                    .as_deref_mut()
                    .expect("an order that paid resources runs in a match with them")
                    .refund(slot, amounts)
        };
        for order in orders.iter() {
            if !matches!(
                order.action,
                Action::CancelTrain { .. } | Action::Rally { .. } | Action::CancelBuild
            ) {
                continue;
            }
            for &unit in order.units {
                let Some(entity) = index.get(unit) else {
                    continue;
                };
                let Ok((owner, queue, site, dead)) = units.get_mut(entity) else {
                    continue;
                };
                if owner.slot() != order.slot {
                    continue;
                }
                match (order.action, queue, site) {
                    (Action::CancelTrain { place }, Some(mut queue), _) => {
                        let place = usize::from(place);
                        let Some(paid) = queue.paid(place) else {
                            continue;
                        };
                        if refunds(paid, order.slot) {
                            queue.remove(place, now);
                        }
                    }
                    (Action::Rally { target }, Some(_), _) => {
                        let target = target.map(|target| match target {
                            RallyTarget::Point { x, z } => {
                                let [x, z] = bounds.clamp_ground([x, z]);
                                RallyTarget::Point { x, z }
                            }
                            unit @ RallyTarget::Unit(_) => unit,
                        });
                        match target {
                            Some(target) => commands.entity(entity).insert(Rally::new(target)),
                            None => commands.entity(entity).remove::<Rally>(),
                        };
                    }
                    (Action::CancelBuild, _, Some(site))
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
        }
    }
}
