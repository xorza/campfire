use bevy_ecs::entity::Entity;
use bevy_ecs::query::{Has, Without};
use bevy_ecs::resource::Resource;
use bevy_ecs::system::{Local, Query, Res, ResMut};
use campfire_sim::{EntityIndex, Position, SimTick, TickInputs};

use crate::actions::action_book::ActionBook;
use crate::actions::action_kind::ActionKind;
use crate::actions::action_slots::ActionSlots;
use crate::actions::action_target::ActionTarget;
use crate::actions::targets::Targets;
use crate::geometry::bounds::Bounds;
use crate::navigation::destination::Destination;
use crate::navigation::group_box::GroupBox;
use crate::navigation::party::{Party, PartyKey};
use crate::orders::order::Action;
use crate::orders::resetting::Resetting;
use crate::orders::tick_orders::TickOrders;
use crate::orders::unit_order::{OrderedUnit, UnitOrder};
use crate::units::dead::Dead;
use crate::units::owner::Owner;
use crate::units::team::Team;

/// The orders the tick's inputs give, in input order, each checked as a player's order needs, for
/// `apply_player_orders` to apply. Not state: it empties within the tick.
#[derive(Resource, Debug, Default)]
pub(super) struct PlayerOrders(Vec<(Entity, UnitOrder)>);

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
    at: Position,
}

/// The orders of the tick's inputs: read once, and the units' orders checked and applied.
#[derive(Debug)]
pub(super) struct OrderInputs;

impl OrderInputs {
    /// Reads the orders of the tick's inputs, once, for each system of orders that applies them.
    pub(super) fn read_orders(inputs: Res<'_, TickInputs>, mut orders: ResMut<'_, TickOrders>) {
        orders.read(&inputs);
    }

    /// Checks each order of the tick, in input order, so a later order in the tick wins, for each
    /// of its units, by stable id. An order to a unit its player does not control, that is dead or
    /// resets, is dropped, and so are a move of a unit with nowhere to walk, an attack on a unit
    /// that is not a living enemy or that none of its weapons selects, a slot's action of a kind
    /// other than a cast, a train or a gather, a gather at no unit, and a build of a slot that
    /// holds none: a client can send anything. A move to two units or more that walk moves them as
    /// a group: each walks to its own goal by the group's box, as one party, the order's.
    pub(super) fn check_player_orders(
        (tick, bounds, orders, index): (
            Res<'_, SimTick>,
            Res<'_, Bounds>,
            Res<'_, TickOrders>,
            Res<'_, EntityIndex>,
        ),
        book: Res<'_, ActionBook>,
        targets: Targets<'_, '_>,
        units: Query<'_, '_, Commanded, (Without<Dead>, Without<Resetting>)>,
        mut checked: ResMut<'_, PlayerOrders>,
        mut movers: Local<'_, Vec<Mover>>,
    ) {
        let now = tick.start();
        for order in orders.iter() {
            if !order.action.to_units() {
                continue;
            }
            let controlled = order
                .units
                .iter()
                .filter_map(|&id| {
                    let entity = index.get(id)?;
                    Some((entity, units.get(entity).ok()?))
                })
                .filter(|(_, (owner, ..))| owner.slot() == order.slot);
            if let Action::Move { x, z } = order.action {
                movers.clear();
                movers.extend(
                    controlled
                        .filter(|(_, (.., walks, _))| *walks)
                        .map(|(entity, (_, _, &at, ..))| Mover { entity, at }),
                );
                let Some(group) = GroupBox::of(movers.iter().map(|mover| mover.at)) else {
                    continue;
                };
                if let &[Mover { entity, .. }] = movers.as_slice() {
                    checked
                        .0
                        .push((entity, UnitOrder::Move { x, z, party: None }));
                    continue;
                }
                let goal = bounds.clamp_ground([x, z]);
                let key = PartyKey::Order {
                    tick: now,
                    slot: order.slot,
                    number: order.number,
                };
                for &Mover { entity, at } in &*movers {
                    let [x, z] = group.goal_of(at, goal, *bounds);
                    let party = Party {
                        key,
                        goal: bounds.ground_point(goal, at),
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
                continue;
            }
            for (entity, (_, team, _, _, slots)) in controlled {
                let unit_order = match order.action {
                    Action::Attack { target } => {
                        let selected = team.and_then(|&team| {
                            let unit = targets.enemy(team, target)?;
                            Some((targets.relation(team, unit.team), unit.tags))
                        });
                        let armed = slots.is_some_and(|slots| {
                            selected.is_some() && book.weapon_for(slots, selected).is_some()
                        });
                        armed.then_some(UnitOrder::Attack { target })
                    }
                    Action::Slot { slot, target } => {
                        let kind = slots.and_then(|slots| slots.kind_in(&book, slot));
                        match (kind, target) {
                            (Some(ActionKind::Cast | ActionKind::Train), _) => {
                                Some(UnitOrder::Slot { slot, target })
                            }
                            (Some(ActionKind::Gather), ActionTarget::Unit(target)) => {
                                Some(UnitOrder::Gather { slot, target })
                            }
                            _ => None,
                        }
                    }
                    Action::Build { slot, target } => {
                        let kind = slots.and_then(|slots| slots.kind_in(&book, slot));
                        (kind == Some(ActionKind::Build))
                            .then_some(UnitOrder::Build { slot, target })
                    }
                    Action::Stop => Some(UnitOrder::Stop),
                    Action::Move { .. }
                    | Action::Learn { .. }
                    | Action::Buy { .. }
                    | Action::Sell { .. }
                    | Action::Swap { .. }
                    | Action::CancelTrain { .. }
                    | Action::Rally { .. }
                    | Action::CancelBuild => {
                        unreachable!("an action that goes to no unit was skipped")
                    }
                };
                checked.0.extend(unit_order.map(|order| (entity, order)));
            }
        }
    }

    /// Applies the tick's checked player orders, in input order, each as every order applies; a
    /// player orders no reset.
    pub(super) fn apply_player_orders(
        tick: Res<'_, SimTick>,
        bounds: Res<'_, Bounds>,
        mut checked: ResMut<'_, PlayerOrders>,
        mut units: Query<'_, '_, OrderedUnit>,
    ) {
        let now = tick.start();
        for (entity, order) in checked.0.drain(..) {
            let parts = units.get_mut(entity).expect("a checked order's unit");
            let resets = order.apply(parts, &bounds, now);
            debug_assert!(!resets, "a player orders no reset");
        }
    }
}
