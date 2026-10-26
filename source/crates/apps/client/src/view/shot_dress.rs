use bevy::asset::AssetServer;
use bevy::ecs::system::{EntityCommands, Res, ResMut, SystemParam};
use bevy::math::Vec3;
use bevy::mesh::Mesh3d;
use bevy::pbr::MeshMaterial3d;
use bevy::transform::components::Transform;
use campfire_units::Variation;
use campfire_values::UnitType;

use crate::view::match_colors::Paint;
use crate::view::model_scenes::ModelScenes;
use crate::view::unit_looks::UnitLooks;
use crate::view::{Palette, SHOT_HEIGHT};

/// What a projectile's drawing wears: its type's look, or the palette's ball for a type of none.
#[derive(SystemParam, Debug)]
pub(crate) struct ShotDress<'w> {
    palette: Res<'w, Palette>,
    looks: Res<'w, UnitLooks>,
    assets: Res<'w, AssetServer>,
    scenes: ResMut<'w, ModelScenes>,
}

impl ShotDress<'_> {
    /// Dresses `root`, the drawing of a projectile of `unit_type` in `variation`: its look's
    /// models, nothing for a look of no models, or with no look a ball at a chest's height above
    /// it. Whether it has a look, whose models turn along its flight.
    pub(crate) fn dress(
        &mut self,
        root: &mut EntityCommands<'_>,
        unit_type: UnitType,
        variation: Option<Variation>,
    ) -> bool {
        let Some(models) = self.looks.look(unit_type, variation) else {
            root.with_child((
                Mesh3d(self.palette.projectile.clone()),
                MeshMaterial3d(self.palette.shot.clone()),
                Transform::from_translation(Vec3::Y * SHOT_HEIGHT),
            ));
            return false;
        };
        for model in models {
            root.with_child(model.bundle(&mut self.scenes, &self.assets, Paint::default()));
        }
        true
    }
}
