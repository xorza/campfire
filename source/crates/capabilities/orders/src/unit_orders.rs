use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Commands, Local, Query, Res, ResMut};
use campfire_actions::{
    ActionBook, ActionKind, ActionSlots, ActionTarget, ProgressIndexes, Targets,
};
use campfire_declare::{Role, Scratch};
use campfire_math::Vec3;
use campfire_navigation::{GroupBox, Party, PartyKey, Route, RouteDues};
use campfire_players::TickCommands;
use campfire_sim::{DueIndex, EntityIndex, Position, SimTick};
use campfire_units::{BuildTarget, Dead, Destination, GroundPoint, MapBook, Owner, UnitOrdered};
use campfire_values::Team;
use campfire_vision::{SeenTargets, TeamSight};

use crate::orders_action::OrdersAction;
use crate::resetting::Resetting;
use crate::unit_order::{OrderedUnit, UnitDues, UnitOrder};

/// The orders the tick's inputs give, in input order, each checked as a player's order needs, for
/// `apply_player_orders` to apply. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub struct PlayerOrders(Vec<(Entity, UnitOrder)>);

/// The parts of a unit a player's order checks.
type Commanded = (
    &'static Owner,
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
    /// that the object's users do not select. A move to two units or more that walk moves them as a
    /// group: each walks to its own goal by the group's box, as one party, the order's.
    pub(super) fn check_player_orders<S: TeamSight>(
        (tick, map, index): (Res<'_, SimTick>, Res<'_, MapBook>, Res<'_, EntityIndex>),
        mut orders: TickCommands<'_, '_, OrdersAction>,
        book: Res<'_, ActionBook>,
        (targets, visible): (Targets<'_, '_>, SeenTargets<'_, '_, S>),
        units: Query<'_, '_, Commanded, (Without<Dead>, Without<Resetting>)>,
        mut checked: ResMut<'_, PlayerOrders>,
        mut movers: Local<'_, Vec<Mover>>,
    ) {
        debug_assert!(checked.is_clear(), "the last tick applied its orders");
        let now = tick.start();
        orders.for_each(|order| {
            let controlled = order
                .units
                .iter()
                .filter_map(|&id| {
                    let entity = index.get(id)?;
                    Some((entity, units.get(entity).ok()?))
                })
                .filter(|(_, (owner, ..))| order.controls(**owner));
            if let OrdersAction::Move {
                to: GroundPoint { x, z },
            } = order.action
            {
                movers.clear();
                movers.extend(controlled.filter(|(_, (.., walks, _))| *walks).map(
                    |(entity, (_, _, &at, ..))| Mover {
                        entity,
                        position: at.get(),
                    },
                ));
                let Some(group) = GroupBox::of(movers.iter().map(|mover| mover.position)) else {
                    return;
                };
                if let &[Mover { entity, .. }] = movers.as_slice() {
                    checked
                        .0
                        .push((entity, UnitOrder::Move { x, z, party: None }));
                    return;
                }
                let goal = map.bounds.clamp_ground([x, z]);
                let key = PartyKey::Order {
                    tick: now,
                    seat: order.seat,
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
                    checked.0.push((
                        entity,
                        UnitOrder::Move {
                            x,
                            z,
                            party: Some(party),
                        },
                    ));
                }
                return;
            }
            for (entity, (_, team, _, _, slots)) in controlled {
                let sees = |target| team.is_some_and(|&team| visible.sees(team, target));
                let unit_order = match order.action {
                    OrdersAction::Attack { target } => {
                        let selected = team.and_then(|&team| {
                            let unit = targets.enemy(team, target)?;
                            Some((targets.relation(team, unit.team), unit.tags))
                        });
                        let armed = slots.is_some_and(|slots| {
                            selected.is_some() && slots.weapon_in(&book, selected).is_some()
                        });
                        (armed && sees(target)).then_some(UnitOrder::Attack { target })
                    }
                    OrdersAction::Slot { slot, target } => {
                        let kind = slots.and_then(|slots| slots.kind_in(&book, slot));
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
                    OrdersAction::Build { slot, target } => {
                        let kind = slots.and_then(|slots| slots.kind_in(&book, slot));
                        let hidden = matches!(target, BuildTarget::Site(site) if !sees(site));
                        (kind == Some(ActionKind::Build) && !hidden)
                            .then_some(UnitOrder::Build { slot, target })
                    }
                    OrdersAction::Use { target } => {
                        sees(target).then_some(UnitOrder::Use { target })
                    }
                    OrdersAction::Stop => Some(UnitOrder::Stop),
                    OrdersAction::Move { .. } => unreachable!("a move was checked as a group"),
                };
                checked.0.extend(unit_order.map(|order| (entity, order)));
            }
        });
    }

    /// Applies the tick's checked player orders, in input order, each as every order applies, and
    /// triggers each as `UnitOrdered`, in the same order; a player orders no reset.
    pub(super) fn apply_player_orders(
        tick: Res<'_, SimTick>,
        map: Res<'_, MapBook>,
        mut checked: ResMut<'_, PlayerOrders>,
        mut units: Query<'_, '_, OrderedUnit>,
        (mut indexes, mut asks): (ProgressIndexes<'_>, ResMut<'_, DueIndex<Route>>),
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
            };
            let order = order.give(parts, &map.bounds, now, &mut dues);
            commands.trigger(UnitOrdered { entity, order });
        }
    }
}

impl Scratch for PlayerOrders {
    fn installed(_: &Role) -> Option<Self> {
        Some(Self::default())
    }

    fn is_clear(&self) -> bool {
        self.0.is_empty()
    }
}
