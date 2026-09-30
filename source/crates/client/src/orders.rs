use bevy::app::{App, Plugin, Update};
use bevy::camera::Camera;
use bevy::ecs::query::With;
use bevy::ecs::system::{Query, Res, ResMut, Single};
use bevy::input::ButtonInput;
use bevy::input::mouse::MouseButton;
use bevy::math::Vec3;
use bevy::math::primitives::InfinitePlane3d;
use bevy::transform::components::GlobalTransform;
use bevy::window::{PrimaryWindow, Window};
use campfire_capabilities::{Action, Order, Owner};
use campfire_math::Num;
use campfire_net::PendingOrders;
use campfire_sim::StableId;
use lightyear::prelude::Predicted;

/// Turns the player's clicks into orders: a right click on the ground walks their hero there.
#[derive(Debug)]
pub(crate) struct Orders;

impl Plugin for Orders {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, Orders::walk_to_click);
    }
}

impl Orders {
    fn walk_to_click(
        buttons: Res<'_, ButtonInput<MouseButton>>,
        window: Single<'_, '_, &Window, With<PrimaryWindow>>,
        camera: Single<'_, '_, (&Camera, &GlobalTransform)>,
        heroes: Query<'_, '_, &StableId, (With<Owner>, With<Predicted>)>,
        mut orders: ResMut<'_, PendingOrders>,
    ) {
        if !buttons.just_pressed(MouseButton::Right) {
            return;
        }
        let (camera, place) = *camera;
        let Some(point) = window
            .cursor_position()
            .and_then(|cursor| camera.viewport_to_world(place, cursor).ok())
            .and_then(|ray| {
                ray.plane_intersection_point(Vec3::ZERO, InfinitePlane3d::new(Vec3::Y))
            })
        else {
            return;
        };
        let (Some(x), Some(z), Ok(&hero)) = (meters(point.x), meters(point.z), heroes.single())
        else {
            return;
        };
        orders.push(Order {
            unit: hero,
            action: Action::Move { x, z },
        });
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
