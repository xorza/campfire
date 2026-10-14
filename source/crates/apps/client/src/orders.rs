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

use crate::own_player::OwnPlayer;
use crate::pointer::Pointer;
use crate::view::ViewSystems;

/// Turns the player's clicks and keys into orders for their avatar: a right click on an enemy
/// attacks it, and anywhere else but the UI walks there; Q, W, E and R cast the abilities of slots 0 to 3,
/// and with Ctrl held learn their next rank; 1 to 6 cast the actions of the inventory's slots; S
/// stops it. A key aims as its slot's action takes: at the unit under the cursor, at the ground
/// point under it, or at nothing (design 13, D11).
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
                .after(ViewSystems),
        );
    }
}

impl Orders {
    fn click(pointer: Pointer<'_, '_>, mut orders: ResMut<'_, PendingOrders>) {
        if pointer.over_ui() {
            return;
        }
        let (Some(avatar), Some(point)) = (pointer.own_avatar(), pointer.ground()) else {
            return;
        };
        let action = match pointer.unit_at(point) {
            Some(unit) if unit.team != avatar.team => OrdersAction::Attack { target: unit.id },
            _ => {
                let (Some(x), Some(z)) = (meters(point.x), meters(point.z)) else {
                    return;
                };
                OrdersAction::Move { x, z }
            }
        };
        orders.push(Order::one(avatar.id, action));
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
            .and_then(|point| pointer.unit_at(point))
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

    fn stop(pointer: Pointer<'_, '_>, mut orders: ResMut<'_, PendingOrders>) {
        if let Some(avatar) = pointer.own_avatar() {
            orders.push(Order::one(avatar.id, OrdersAction::Stop));
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

/// A point of the ground as a sim position, each axis to the millimeter; `None` past the bound.
fn position(point: Vec3) -> Option<Position> {
    let axes = [point.x, point.y, point.z].map(meters);
    let [Some(x), Some(y), Some(z)] = axes else {
        return None;
    };
    Position::new(SimVec3::new(x, y, z))
}

/// A clicked coordinate as a sim number, to the millimeter; `None` far off any map.
#[expect(
    clippy::cast_possible_truncation,
    reason = "the value is rounded and checked against the i64 range first"
)]
fn meters(value: f32) -> Option<Num> {
    let millimeters = (f64::from(value) * 1000.0).round();
    if !(-1e12..=1e12).contains(&millimeters) {
        return None;
    }
    Num::from_int(millimeters as i64)?.checked_div_int(1000)
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
        // A ground point to the millimeter, its height among its axes.
        assert_eq!(
            position(Vec3::new(3.0, 0.0, -2.0)),
            Some(Position::from((3, 0, -2)))
        );
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
}
