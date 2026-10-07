use bevy::app::{App, Plugin, Update};
use bevy::ecs::system::{Res, ResMut};
use bevy::input::ButtonInput;
use bevy::input::keyboard::KeyCode;
use bevy::input::mouse::MouseButton;
use campfire_capabilities::{Action, ActionTarget, Order};
use campfire_math::Num;
use campfire_net::PendingOrders;

use crate::pointer::Pointer;

/// Turns the player's clicks and keys into orders for their avatar: a right click on an enemy
/// attacks it, and anywhere else walks there; Q, W, E and R cast the abilities of slots 0 to 3,
/// at the unit under the cursor when there is one, and with Ctrl held learn their next rank; S
/// stops it. The sim ignores a target an ability does not take, so a key needs no knowledge of
/// the ability.
#[derive(Debug)]
pub(crate) struct Orders;

/// The keys that cast, by ability slot.
const CAST_KEYS: [KeyCode; 4] = [KeyCode::KeyQ, KeyCode::KeyW, KeyCode::KeyE, KeyCode::KeyR];
/// The keys that, held, make a cast key learn.
const LEARN_KEYS: [KeyCode; 2] = [KeyCode::ControlLeft, KeyCode::ControlRight];
/// The key that stops.
const STOP_KEY: KeyCode = KeyCode::KeyS;

impl Plugin for Orders {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, (Orders::click, Orders::cast, Orders::stop));
    }
}

impl Orders {
    fn click(
        buttons: Res<'_, ButtonInput<MouseButton>>,
        pointer: Pointer<'_, '_>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        if !buttons.just_pressed(MouseButton::Right) {
            return;
        }
        let (Some(avatar), Some(point)) = (pointer.own_avatar(), pointer.ground()) else {
            return;
        };
        let action = match pointer.unit_at(point) {
            Some(unit) if unit.team != avatar.team => Action::Attack { target: unit.id },
            _ => {
                let (Some(x), Some(z)) = (meters(point.x), meters(point.z)) else {
                    return;
                };
                Action::Move { x, z }
            }
        };
        orders.push(Order::one(avatar.id, action));
    }

    fn cast(
        keys: Res<'_, ButtonInput<KeyCode>>,
        pointer: Pointer<'_, '_>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        let Some(avatar) = pointer.own_avatar() else {
            return;
        };
        let learn = keys.any_pressed(LEARN_KEYS);
        for (slot, &key) in (0..).zip(&CAST_KEYS) {
            if !keys.just_pressed(key) {
                continue;
            }
            let under = pointer.ground().and_then(|point| pointer.unit_at(point));
            let target = under.map_or(ActionTarget::None, |unit| ActionTarget::Unit(unit.id));
            orders.push(Order::one(
                avatar.id,
                Orders::slot_action(slot, learn, target),
            ));
        }
    }

    fn stop(
        keys: Res<'_, ButtonInput<KeyCode>>,
        pointer: Pointer<'_, '_>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        if !keys.just_pressed(STOP_KEY) {
            return;
        }
        if let Some(avatar) = pointer.own_avatar() {
            orders.push(Order::one(avatar.id, Action::Stop));
        }
    }

    /// What a cast key orders of `slot`: with Ctrl held, a learn, and else a cast at `target`.
    const fn slot_action(slot: u8, learn: bool, target: ActionTarget) -> Action {
        if learn {
            Action::Learn { slot }
        } else {
            Action::Slot { slot, target }
        }
    }
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
    use super::*;

    #[test]
    fn a_cast_key_casts_at_its_target_and_with_ctrl_learns() {
        let target = ActionTarget::None;
        assert_eq!(
            Orders::slot_action(2, false, target),
            Action::Slot { slot: 2, target }
        );
        assert_eq!(
            Orders::slot_action(2, true, target),
            Action::Learn { slot: 2 }
        );
    }
}
