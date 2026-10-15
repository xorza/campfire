use std::slice;

use bevy::app::{App, Plugin, Update};
use bevy::ecs::query::Allow;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Query, Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::common_conditions::input_just_pressed;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use bevy::math::Vec3;
use campfire_actions::{ActionBook, ActionSlots, ActionTarget, Aim, UseGroup};
use campfire_capabilities::{Action, Order};
use campfire_math::{Num, Vec3 as SimVec3};
use campfire_net::PendingOrders;
use campfire_orders::OrdersAction;
use campfire_progression::ProgressionAction;
use campfire_sim::{Position, StableId, Unpredicted};
use campfire_values::Team;

use crate::own_player::OwnPlayer;
use crate::pointer::{Pointed, Pointer};
use crate::selection::{Selection, SelectionSystems};

/// Turns the player's clicks and keys into orders: a right click orders the selection, as one
/// group order, to attack an enemy under the cursor, and anywhere else but the UI to walk there,
/// and S stops it (design 17, D7); Q, W, E and R cast the avatar's abilities of slots 0 to 3, and
/// with Ctrl held learn their next rank; 1 to 6 cast the actions of its inventory's slots. A key
/// aims as its slot's action takes: at the unit under the cursor, at the ground point under it,
/// or at nothing (design 13, D11).
#[derive(Debug)]
pub(crate) struct Orders;

/// A unit's slots, and the slot kind of its inventory's, if it carries one.
type Casters<'w, 's> =
    Query<'w, 's, (&'static ActionSlots, Option<&'static UseGroup>), Allow<Unpredicted>>;

/// The keys that cast, by ability slot.
const CAST_KEYS: [KeyCode; 4] = [KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE, KeyCode::KeyR];
/// The keys that cast the actions of the inventory's slots, by inventory slot.
const ITEM_KEYS: [KeyCode; 6] = [
    KeyCode::Digit1,
    KeyCode::Digit2,
    KeyCode::Digit3,
    KeyCode::Digit4,
    KeyCode::Digit5,
    KeyCode::Digit6,
];
/// The keys that, held, make a cast key learn.
const LEARN_KEYS: [KeyCode; 2] = [KeyCode::ControlLeft, KeyCode::ControlRight];
/// The key that stops.
const STOP_KEY: KeyCode = KeyCode::KeyS;

impl Plugin for Orders {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                Orders::click.run_if(input_just_pressed(MouseButton::Right)),
                Orders::cast.run_if(Orders::cast_pressed),
                Orders::stop.run_if(input_just_pressed(STOP_KEY)),
            )
                .after(SelectionSystems),
        );
    }
}

impl Orders {
    fn click(
        pointer: Pointer<'_, '_>,
        own: OwnPlayer<'_, '_>,
        selection: Res<'_, Selection>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        if pointer.over_ui() {
            return;
        }
        let Some(point) = pointer.ground() else {
            return;
        };
        let under = pointer.unit_at(point, selection.get());
        let order = Orders::click_action(under, own.team(), point)
            .and_then(|action| selection.order(action));
        if let Some(order) = order {
            orders.push(order);
        }
    }

    fn cast(
        keys: Res<'_, ButtonInput<KeyCode>>,
        pointer: Pointer<'_, '_>,
        own: OwnPlayer<'_, '_>,
        (book, avatars): (Res<'_, ActionBook>, Casters<'_, '_>),
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(avatar) = own.avatar() else {
            return;
        };
        let Ok((slots, group)) = avatars.get(avatar.entity) else {
            return;
        };
        let ground = pointer.ground();
        let under = ground
            .and_then(|point| pointer.unit_at(point, slice::from_ref(&avatar.id)))
            .map(|unit| unit.id);
        let point = ground.and_then(position);
        let learn = keys.any_pressed(LEARN_KEYS);
        let abilities = (0..).zip(&CAST_KEYS).map(|(slot, key)| (slot, key, learn));
        // An item key past the inventory's slots casts nothing.
        let inventory = group.map(|group| {
            let kind = group.kind();
            let count = slots.iter().filter(|held| held.kind == kind).count();
            (slots.first_of(kind), count.min(ITEM_KEYS.len()))
        });
        let items = inventory.into_iter().flat_map(|(first, count)| {
            (first..)
                .zip(&ITEM_KEYS[..count])
                .map(|(slot, key)| (slot, key, false))
        });
        for (slot, &key, learn) in abilities.chain(items) {
            if !keys.just_pressed(key) {
                continue;
            }
            let action = slots.slot(slot).and_then(|held| held.action);
            let aim = action
                .and_then(|action| book.get(action))
                .map(|action| &action.aim);
            let target = Takes::of(aim).target(under, point);
            orders.push(Order::one(
                avatar.id,
                Orders::slot_action(slot, learn, target),
            ));
        }
    }

    fn stop(selection: Res<'_, Selection>, mut orders: ResMut<'_, PendingOrders>) {
        if let Some(order) = selection.order(OrdersAction::Stop) {
            orders.push(order);
        }
    }

    /// What a right click at `point` of the ground orders a player of `team`: an attack on
    /// `under`, the unit under the cursor, when it is of another team, else a move to the point;
    /// none far off any map.
    fn click_action(
        under: Option<Pointed>,
        team: Option<Team>,
        point: Vec3,
    ) -> Option<OrdersAction> {
        match under {
            Some(unit) if team.is_some_and(|team| unit.team != team) => {
                Some(OrdersAction::Attack { target: unit.id })
            }
            _ => Some(OrdersAction::Move {
                x: Num::try_from(point.x).ok()?,
                z: Num::try_from(point.z).ok()?,
            }),
        }
    }

    /// Whether a cast key or an item key went down this frame.
    fn cast_pressed(keys: Res<'_, ButtonInput<KeyCode>>) -> bool {
        keys.any_just_pressed(CAST_KEYS) || keys.any_just_pressed(ITEM_KEYS)
    }

    /// What a cast key orders of `slot`: with Ctrl held, a learn, and else a cast at `target`.
    const fn slot_action(slot: u8, learn: bool, target: ActionTarget) -> Action {
        if learn {
            Action::Progression(ProgressionAction::Learn { slot })
        } else {
            Action::Orders(OrdersAction::Slot { slot, target })
        }
    }
}

/// What a key's action takes as its target: a unit, a point, a direction's among them, or
/// nothing, as no action does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Takes {
    Unit,
    Point,
    Nothing,
}

impl Takes {
    /// What an action that aims as `aim` says takes; nothing for no action.
    const fn of(aim: Option<&Aim>) -> Takes {
        match aim {
            Some(Aim::Unit(_)) => Takes::Unit,
            Some(Aim::Point { .. } | Aim::Direction) => Takes::Point,
            Some(Aim::None) | None => Takes::Nothing,
        }
    }

    /// The target it takes, `under` the unit under the cursor and `point` the ground point under
    /// it: none when the cursor shows none, which the sim refuses of an action that needs one.
    fn target(self, under: Option<StableId>, point: Option<Position>) -> ActionTarget {
        let target = match self {
            Takes::Unit => under.map(ActionTarget::Unit),
            Takes::Point => point.map(ActionTarget::Point),
            Takes::Nothing => None,
        };
        target.unwrap_or(ActionTarget::None)
    }
}

/// A point of the ground as a sim position, each axis exactly as `Num::try_from` takes it;
/// `None` past the bound.
fn position(point: Vec3) -> Option<Position> {
    let axes = [point.x, point.y, point.z].map(|axis| Num::try_from(axis).ok());
    let [Some(x), Some(y), Some(z)] = axes else {
        return None;
    };
    Position::new(SimVec3::new(x, y, z))
}

#[cfg(test)]
mod tests {
    use campfire_sim::IdAllocator;

    use super::*;

    #[test]
    fn a_key_aims_as_its_action_takes() {
        let unit = IdAllocator::default().allocate();
        let point = Position::from((3, 0, -2));
        // An action of a point, or of a direction, takes the point; of none, or no action,
        // nothing.
        let aims = [
            (Aim::Point { clamp: false }, Takes::Point),
            (Aim::Direction, Takes::Point),
            (Aim::None, Takes::Nothing),
        ];
        for (aim, takes) in aims {
            assert_eq!(Takes::of(Some(&aim)), takes, "{aim:?}");
        }
        assert_eq!(Takes::of(None), Takes::Nothing);
        // With a unit and a point under the cursor: the unit, the point, or nothing; with
        // neither under it, nothing.
        for (takes, both) in [
            (Takes::Unit, ActionTarget::Unit(unit)),
            (Takes::Point, ActionTarget::Point(point)),
            (Takes::Nothing, ActionTarget::None),
        ] {
            assert_eq!(takes.target(Some(unit), Some(point)), both, "{takes:?}");
            assert_eq!(takes.target(None, None), ActionTarget::None, "{takes:?}");
        }
        // A ground point, exactly, its height among its axes: 0.1 in `f32` is 13,421,773 · 2⁻²⁷,
        // which rounds to the step 1,677,722 of 2⁻²⁴, not a millimeter's multiple.
        assert_eq!(
            position(Vec3::new(3.0, 0.0, -2.0)),
            Some(Position::from((3, 0, -2)))
        );
        let tenth = position(Vec3::new(0.1, 0.0, 0.0)).unwrap();
        assert_eq!(tenth.get().x.to_bits(), 1_677_722);
        assert_eq!(position(Vec3::new(1e15, 0.0, 0.0)), None);
    }

    #[test]
    fn a_cast_key_casts_at_its_target_and_with_ctrl_learns() {
        let target = ActionTarget::None;
        assert_eq!(
            Orders::slot_action(2, false, target),
            Action::Orders(OrdersAction::Slot { slot: 2, target })
        );
        assert_eq!(
            Orders::slot_action(2, true, target),
            Action::Progression(ProgressionAction::Learn { slot: 2 })
        );
    }

    #[test]
    fn a_right_click_attacks_an_enemy_under_the_cursor_and_else_walks_to_the_ground_point() {
        let mut ids = IdAllocator::default();
        let (north, south) = (Team::new(0), Team::new(1));
        let mut unit = |team| Pointed {
            id: ids.allocate(),
            team,
            owner: None,
        };
        let (enemy, friend) = (unit(south), unit(north));
        let point = Vec3::new(3.0, 1.0, -2.5);
        // To (3, −2.5): x = 3, z = −5 / 2, the height not sent.
        let walk = Some(OrdersAction::Move {
            x: Num::int(3),
            z: Num::int(-5) / Num::int(2),
        });
        let click = |under, team| Orders::click_action(under, team, point);
        assert_eq!(
            click(Some(enemy), Some(north)),
            Some(OrdersAction::Attack { target: enemy.id })
        );
        assert_eq!(click(Some(friend), Some(north)), walk);
        assert_eq!(click(None, Some(north)), walk);
        // A client of no team has no enemy.
        assert_eq!(click(Some(enemy), None), walk);
        // A ground point far off any map orders nothing.
        let far = Vec3::new(1e15, 0.0, 0.0);
        assert_eq!(Orders::click_action(None, Some(north), far), None);
    }
}
