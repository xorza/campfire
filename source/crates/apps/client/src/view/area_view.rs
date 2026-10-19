use bevy::app::{App, Plugin, Startup, Update};
use bevy::asset::{Assets, Handle};
use bevy::camera::visibility::Visibility;
use bevy::color::Color;
use bevy::ecs::entity::Entity;
use bevy::ecs::query::{Allow, Has, With, Without};
use bevy::ecs::resource::Resource;
use bevy::ecs::schedule::IntoScheduleConfigs;
use bevy::ecs::system::{Commands, Query, Res, ResMut};
use bevy::material::AlphaMode;
use bevy::math::Vec3;
use bevy::math::primitives::Cylinder;
use bevy::mesh::{Mesh, Mesh3d, Meshable};
use bevy::pbr::{MeshMaterial3d, StandardMaterial};
use bevy::time::Time;
use bevy::transform::components::Transform;
use campfire_areas::{Area, AreaReach};
use campfire_sim::{Position, Unpredicted};
use campfire_values::UnitType;

use crate::view::client_data::ClientData;
use crate::view::drawing::{Drawing, DrawingOf};
use crate::view::float_num::FloatNum;
use crate::view::glide::{Follows, Glide, Spot};
use crate::view::{DrawSystems, ground};

/// Draws each area the client receives, of a mode with areas: a see-through disc of its radius
/// on the ground, which glides as a unit's drawing does.
#[derive(Debug)]
pub(crate) struct AreaView;

/// The disc of radius 1 an area's drawing scales to its radius, and its see-through tint, which
/// shows the ground under it.
#[derive(Resource, Debug)]
struct AreaPalette {
    disc: Handle<Mesh>,
    reach: Handle<StandardMaterial>,
}

/// The areas not drawn yet: where each lies, its type, and whether the client only receives it.
type NewAreas<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static Position,
        &'static UnitType,
        Has<Unpredicted>,
    ),
    (With<Area>, Without<Drawing>, Allow<Unpredicted>),
>;

/// An area's disc: how thick it is, and how far over the ground it lies, so the ground does not
/// hide it.
const AREA_THICKNESS: f32 = 0.02;
pub(crate) const AREA_LIFT: f32 = 0.03;

impl Plugin for AreaView {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, Self::set_palette);
        app.add_systems(Update, Self::draw_areas.in_set(DrawSystems));
    }
}

impl AreaView {
    fn set_palette(
        mut commands: Commands<'_, '_>,
        mut meshes: ResMut<'_, Assets<Mesh>>,
        mut materials: ResMut<'_, Assets<StandardMaterial>>,
    ) {
        commands.insert_resource(AreaPalette {
            disc: meshes.add(Cylinder::new(1.0, AREA_THICKNESS).mesh()),
            reach: materials.add(StandardMaterial {
                base_color: Color::srgba(1.0, 0.6, 0.2, 0.35),
                alpha_mode: AlphaMode::Blend,
                ..StandardMaterial::default()
            }),
        });
    }

    /// Gives each area the client received a drawing where it lies: a see-through disc of its
    /// radius on the ground, which glides as a unit's drawing does.
    fn draw_areas(
        palette: Res<'_, AreaPalette>,
        time: Res<'_, Time>,
        areas: NewAreas<'_, '_>,
        reach: AreaReach<'_>,
        data: Res<'_, ClientData>,
        mut commands: Commands<'_, '_>,
    ) {
        let heights = data.lifts();
        for (area, &pos, &unit_type, received) in &areas {
            let radius = reach
                .radius(unit_type)
                .expect("an area's type has its reach")
                .float();
            let disc = commands
                .spawn((
                    Mesh3d(palette.disc.clone()),
                    MeshMaterial3d(palette.reach.clone()),
                    Transform::from_translation(Vec3::Y * AREA_LIFT)
                        .with_scale(Vec3::new(radius, 1.0, radius)),
                ))
                .id();
            let at = ground(pos, heights);
            commands
                .spawn((
                    DrawingOf(area),
                    Transform::from_translation(at),
                    Visibility::default(),
                    Glide::rest(
                        Spot {
                            point: at,
                            yaw: 0.0,
                        },
                        Follows::of(received),
                        time.elapsed(),
                    ),
                ))
                .add_child(disc);
        }
    }
}
