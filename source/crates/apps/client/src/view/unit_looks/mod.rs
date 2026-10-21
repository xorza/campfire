use std::sync::Arc;

use bevy::asset::RenderAssetUsages;
use bevy::asset::{AssetServer, Handle};
use bevy::ecs::bundle::Bundle;
use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use bevy::gltf::{GltfAssetLabel, GltfLoaderSettings};
use bevy::math::Vec3;
use bevy::transform::components::Transform;
use bevy::world_serialization::{WorldAsset, WorldAssetRoot};
use campfire_client_files::{ClientUnit, ModelTop};
use campfire_math::Num;
use campfire_units::TypeOriginBook;
use campfire_values::{NameId, PackageIndex, UnitType};

use crate::view::client_data::ClientData;
use crate::view::match_colors::Paint;
use crate::view::package_source::PackageAssets;

/// Each unit type's look, as its package's client data gives it, by type: none for a type of no
/// look, drawn as the engine's own shapes are.
#[derive(Resource, Debug, Default)]
pub(crate) struct UnitLooks {
    looks: Vec<Option<TypeLook>>,
}

/// A type's look: its models, none for a look drawn as nothing, the top of the highest of them in
/// meters, none when the file says no model's, and the angle in degrees a build preview places
/// it at.
#[derive(Debug)]
struct TypeLook {
    models: Vec<LookModel>,
    top: Option<f32>,
    placement_angle: Num,
}

/// A model a look is drawn with: its glTF's first scene, its nodes, and the factor it is drawn
/// at.
#[derive(Debug, Clone)]
pub(crate) struct LookModel {
    pub(crate) scene: Handle<WorldAsset>,
    pub(crate) nodes: LookNodes,
    pub(crate) scale: f32,
}

/// What a drawn model needs once its scene is spawned: the names of the nodes its look hides, and
/// the name of the node its look turns by its unit's turret angle, if it has one.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct LookNodes {
    pub(crate) hide: Arc<[String]>,
    pub(crate) turret: Option<Arc<str>>,
}

impl UnitLooks {
    /// The looks of every type of `origins`: the models of the type's key in its package's units.
    pub(crate) fn of(origins: &TypeOriginBook, data: &ClientData, assets: &AssetServer) -> Self {
        let looks = origins
            .iter()
            .map(|(_, origin)| {
                let package = origin.package;
                data.of(package)
                    .units
                    .units
                    .get(&*origin.name)
                    .map(|unit| TypeLook {
                        models: LookModel::all(package, unit, assets),
                        top: unit.top().map(ModelTop::get),
                        placement_angle: unit.placement_angle.unwrap_or(Num::ZERO),
                    })
            })
            .collect();
        Self { looks }
    }

    /// The models of `unit_type`'s look; `None` for a type of no look in the client data.
    pub(crate) fn look(&self, unit_type: UnitType) -> Option<&[LookModel]> {
        let look = self.looks.get(unit_type.index())?.as_ref()?;
        Some(&look.models)
    }

    /// The top in meters of the highest model of `unit_type`'s look, as the import measured it;
    /// none for a type of no look, or of a look whose file gives no model's.
    pub(crate) fn top(&self, unit_type: UnitType) -> Option<f32> {
        self.looks.get(unit_type.index())?.as_ref()?.top
    }

    /// The angle in degrees a build preview places `unit_type` at: its look's, 0 for a look that
    /// gives none or a type of no look.
    pub(crate) fn placement_angle(&self, unit_type: UnitType) -> Num {
        let look = self.looks.get(unit_type.index()).and_then(Option::as_ref);
        look.map_or(Num::ZERO, |look| look.placement_angle)
    }
}

impl LookModel {
    /// What a drawing's child of this model holds: its scene, its nodes, its scale and `paint`.
    pub(crate) fn bundle(&self, paint: Paint) -> impl Bundle {
        (
            WorldAssetRoot(self.scene.clone()),
            self.nodes.clone(),
            Transform::from_scale(Vec3::splat(self.scale)),
            paint,
        )
    }

    /// The models of `look`, a look of the package at `package`, each loading, its meshes kept
    /// on the GPU alone: the client reads no mesh's vertices, as the import measures each look's
    /// top (02-engine-core.md, The import measures a look's height).
    pub(crate) fn all(package: PackageIndex, look: &ClientUnit, assets: &AssetServer) -> Vec<Self> {
        look.models
            .iter()
            .map(|model| Self {
                scene: assets
                    .load_builder()
                    .with_settings(|settings: &mut GltfLoaderSettings| {
                        settings.load_meshes = RenderAssetUsages::RENDER_WORLD;
                    })
                    .load(
                        GltfAssetLabel::Scene(0)
                            .from_asset(PackageAssets::asset_path(package, &model.model)),
                    ),
                nodes: LookNodes {
                    hide: model.hide.as_slice().into(),
                    turret: look.turret.as_deref().map(Arc::from),
                },
                scale: model.scale.get(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
