use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Changed, Has, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_actions::{
    ActionBook, ActionKind, ActionProgress, ActionSlots, ActionTarget, ProgressIndexes, TargetTest,
    Targets,
};
use campfire_declarations::{Role, Scratch};
use campfire_math::{Num, Vec3};
use campfire_navigation::{GroupBox, Party, PartyKey, Route, RouteDues};
use campfire_players::{OrderedUnits, TickCommands};
use campfire_sim::{DueIndex, Position, SimTick};
use campfire_units::{BuildTarget, Dead, Destination, GroundPoint, MapBook, UnitOrdered};
use campfire_values::Team;
use campfire_vision::{SeenTargets, TeamSight};

use crate::attack_move::{AttackMove, GoalDues};
use crate::orders_directive::OrdersDirective;
use crate::resetting::Resetting;
use crate::unit_order::{OrderedUnit, UnitDues, UnitOrder};

/// The orders the tick's inputs give, in input order, each checked as a player's order needs, for
/// `apply_player_orders` to apply. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct OrderQueue(Vec<(Entity, UnitOrder)>);

/// The parts of a unit a player's order checks.
type Commanded = (
    Option<&'static Team>,
    &'static Position,
    Has<Destination>,
    Option<&'static ActionSlots>,
);

/// A unit of a group's move: its entity and where it stands.
#[derive(Debug, Clone, Copy)]
pub(super) struct Mover {
    entity: Entity,
    position: Vec3,
}

/// How a walk of a group's order walks: as a move, or as an attack-move, whose units' scans look
/// on the way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Walk {
    Move,
    AttackMove,
}

impl Walk {
    /// The order of one unit to walk to `x`, `z` so, as one of `party` when a group walks.
    const fn order(self, x: Num, z: Num, party: Option<Party>) -> UnitOrder {
        match self {
            Self::Move => UnitOrder::Move { x, z, party },
            Self::AttackMove => UnitOrder::AttackMove { x, z, party },
        }
    }
}

/// The orders of `orders` among the tick's inputs: each unit's checked, then applied.
#[derive(Debug)]
pub struct UnitOrders;

impl UnitOrders {
    /// Checks each order of the tick, in input order, so a later order in the tick wins, for each
    /// of its units, by stable id. An order to a unit its player does not control, that is dead or
    /// resets, is dropped, and so are a move of a unit with nowhere to walk, an attack on a unit
    /// that is not a living enemy or that none of its weapons selects, a slot's action of a kind
    /// other than a cast, a train or a gather, a gather at no unit, a build of a slot that holds
    /// none, and an order whose unit target its player's vision group does not see: a client can
    /// send anything. A use goes on to `interaction`, which drops one by a unit with no use slot or
    /// that the object's users do not select. A move or an attack-move to two units or more that
    /// walk moves them as a group: each walks to its own goal by the group's box, as one party,
    /// the order's.
    pub(super) fn check_player_orders<S: TeamSight>(
        (tick, map): (Res<'_, SimTick>, Res<'_, MapBook>),
        mut orders: TickCommands<'_, '_, OrdersDirective>,
        book: Res<'_, ActionBook>,
        (targets, visible): (Targets<'_, '_>, SeenTargets<'_, '_, S>),
        units: OrderedUnits<'_, '_, Commanded, (Without<Dead>, Without<Resetting>)>,
        mut checked: ResMut<'_, OrderQueue>,
        mut movers: Local<'_, Vec<Mover>>,
    ) {
        debug_assert!(checked.is_clear(), "the last tick applied its orders");
        let now = tick.start();
        orders.for_each(|order| {
            let controlled = units.controlled(&order);
            let walk = match order.directive {
                OrdersDirective::Move { to } => Some((to, Walk::Move)),
                OrdersDirective::AttackMove { to } => Some((to, Walk::AttackMove)),
                _ => None,
            };
            if let Some((GroundPoint { x, z }, walk)) = walk {
                movers.clear();
                movers.extend(controlled.filter(|(.., (_, _, walks, _))| *walks).map(
                    |(_, entity, (_, &at, ..))| Mover {
                        entity,
                        position: at.get(),
                    },
                ));
                let Some(group) = GroupBox::of(movers.iter().map(|mover| mover.position)) else {
                    return;
                };
                if let &[Mover { entity, .. }] = movers.as_slice() {
                    checked.0.push((entity, walk.order(x, z, None)));
                    return;
                }
                let goal = map.bounds.clamp_ground([x, z]);
                let key = PartyKey::Order {
                    tick: now,
                    player_slot: order.player_slot,
                    number: order.number,
                };
                for &Mover {
                    entity,
                    position: at,
                } in &*movers
                {
                    let [x, z] = group.goal_of(at, goal, map.bounds);
                    let party = Party {
                        key,
                        goal: map.bounds.ground_point(goal, at),
                    };
                    checked.0.push((entity, walk.order(x, z, Some(party))));
                }
                return;
            }
            for (_, entity, (team, _, _, slots)) in controlled {
                let sees = |target| team.is_some_and(|&team| visible.sees(team, target));
                let unit_order = match order.directive {
                    OrdersDirective::Attack { target } => {
                        let armed = team.zip(slots).is_some_and(|(&team, slots)| {
                            let test = TargetTest {
                                book: &book,
                                targets: &targets,
                                team,
                            };
                            slots.may_attack(&test, target)
                        });
                        (armed && sees(target)).then_some(UnitOrder::Attack { target })
                    }
                    OrdersDirective::Slot { slot, target } => {
                        let kind = slots
                            .and_then(|slots| slots.learned(slot))
                            .and_then(|held| held.kind_in(&book));
                        let hidden = matches!(target, ActionTarget::Unit(unit) if !sees(unit));
                        match (kind, target) {
                            _ if hidden => None,
                            (Some(ActionKind::Cast | ActionKind::Train), _) => {
                                Some(UnitOrder::Slot { slot, target })
                            }
                            (Some(ActionKind::Gather), ActionTarget::Unit(target)) => {
                                Some(UnitOrder::Gather { slot, target })
                            }
                            _ => None,
                        }
                    }
                    OrdersDirective::Build { slot, target } => {
                        let kind = slots
                            .and_then(|slots| slots.learned(slot))
                            .and_then(|held| held.kind_in(&book));
                        let hidden = matches!(target, BuildTarget::Site(site) if !sees(site));
                        (kind == Some(ActionKind::Build) && !hidden)
                            .then_some(UnitOrder::Build { slot, target })
                    }
                    OrdersDirective::Use { target } => {
                        sees(target).then_some(UnitOrder::Use { target })
                    }
                    OrdersDirective::Stop => Some(UnitOrder::Stop),
                    OrdersDirective::Move { .. } | OrdersDirective::AttackMove { .. } => {
                        unreachable!("a walk was checked as a group")
                    }
                };
                checked.0.extend(unit_order.map(|order| (entity, order)));
            }
        });
    }

    /// Gives each unit whose wind-down ended since the last run the order it holds, as every order
    /// is given, and triggers it as `UnitOrdered`; a unit that died drops it. It runs before the
    /// tick's orders, so one of them comes after it, as the last order wins.
    pub(super) fn release_held(
        tick: Res<'_, SimTick>,
        map: Res<'_, MapBook>,
        mut units: Query<'_, '_, (OrderedUnit, Has<Dead>), Changed<ActionProgress>>,
        (mut indexes, mut asks, mut goals): (
            ProgressIndexes<'_>,
            ResMut<'_, DueIndex<Route>>,
            ResMut<'_, DueIndex<AttackMove>>,
        ),
        mut commands: Commands<'_, '_>,
    ) {
        let now = tick.start();
        for (mut parts, dead) in &mut units {
            let Some(held) = parts.held.as_mut() else {
                continue;
            };
            let Some(order) = held.get() else {
                continue;
            };
            let winds_down = parts
                .underway
                .as_deref()
                .is_some_and(ActionProgress::winds_down);
            if winds_down && !dead {
                continue;
            }
            held.drop_order();
            if dead {
                continue;
            }
            let (id, entity) = (*parts.id, parts.entity);
            let mut dues = UnitDues {
                progress: indexes.of(id, entity),
                routes: RouteDues::new(&mut asks, id, entity),
                goals: GoalDues::new(&mut goals, id, entity),
            };
            if let Some(order) = order.give(parts, &map.bounds, now, &mut dues) {
                commands.trigger(UnitOrdered { entity, order });
            }
        }
    }

    /// Applies the tick's checked player orders, in input order, each as every order applies, and
    /// triggers each it gives as `UnitOrdered`, in the same order; a player orders no reset.
    pub(super) fn apply_player_orders(
        tick: Res<'_, SimTick>,
        map: Res<'_, MapBook>,
        mut checked: ResMut<'_, OrderQueue>,
        mut units: Query<'_, '_, OrderedUnit>,
        (mut indexes, mut asks, mut goals): (
            ProgressIndexes<'_>,
            ResMut<'_, DueIndex<Route>>,
            ResMut<'_, DueIndex<AttackMove>>,
        ),
        mut commands: Commands<'_, '_>,
    ) {
        let now = tick.start();
        for (entity, order) in checked.0.drain(..) {
            debug_assert_ne!(order, UnitOrder::Reset, "a player orders no reset");
            let parts = units.get_mut(entity).expect("a checked order's unit");
            let id = *parts.id;
            let mut dues = UnitDues {
                progress: indexes.of(id, entity),
                routes: RouteDues::new(&mut asks, id, entity),
                goals: GoalDues::new(&mut goals, id, entity),
            };
            if let Some(order) = order.give(parts, &map.bounds, now, &mut dues) {
                commands.trigger(UnitOrdered { entity, order });
            }
        }
    }
}

impl Scratch for OrderQueue {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
