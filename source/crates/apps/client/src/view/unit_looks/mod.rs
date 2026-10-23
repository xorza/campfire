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
use campfire_client_files::{DrawnLook, ModelTop};
use campfire_declarations::ById;
use campfire_math::Num;
use campfire_units::{TypeOriginBook, Variation, Variations};
use campfire_values::{NameId, PackageIndex, UnitType};
use tracing::error;

use crate::view::client_data::ClientLoad;
use crate::view::match_colors::Paint;
use crate::view::package_source::PackageAssets;

/// Each unit type's look, as its package's client data gives it, by type: none for a type of no
/// look, drawn as the engine's own shapes are.
#[derive(Resource, Debug, Default)]
pub(crate) struct UnitLooks {
    looks: Vec<Option<TypeLook>>,
}

/// The match's unit types, whose looks load: the origin of each, and its variations, none in a
/// match that holds no book of them.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LookTypes<'a> {
    pub(crate) origins: &'a TypeOriginBook,
    pub(crate) variations: Option<&'a ById<UnitType, Variations>>,
}

/// A type's look: its own models, those of each of its variations, none for a look that does not
/// vary, and the angle in degrees a build preview places it at.
#[derive(Debug)]
struct TypeLook {
    own: DrawnModels,
    variations: Box<[DrawnModels]>,
    placement_angle: Num,
}

/// The models a unit draws, none for a look drawn as nothing, and the top of the highest of them
/// in meters, none when the file says no model's.
#[derive(Debug)]
struct DrawnModels {
    models: Vec<LookModel>,
    top: Option<f32>,
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
    /// The looks of every type of `types`: the models of the type's key in its package's units,
    /// and of each of its look's variations; a look whose variations are not as many as its
    /// type's is logged, and each of its units draws its own models
    /// (02-engine-core.md, A look varies by its unit's variation).
    pub(crate) fn of(types: LookTypes<'_>, data: &ClientLoad, assets: &AssetServer) -> Self {
        let looks = types
            .origins
            .iter()
            .map(|(unit_type, origin)| {
                let package = origin.package;
                let unit = data.of(package).units.units.get(&*origin.name)?;
                let load = |drawn| DrawnModels::load(package, drawn, assets);
                let of_type = types.variations.and_then(|book| book.get(unit_type));
                let count = of_type.map_or(0, Variations::len);
                let fits = unit.variations.is_empty() || unit.variations.len() == count;
                if !fits {
                    let (look, looks) = (&*origin.name, unit.variations.len());
                    error!(look, looks, count, "a look's variations are not its type's");
                }
                let variations = unit.variations.iter().filter(|_| fits);
                Some(TypeLook {
                    own: load(unit.own()),
                    variations: variations
                        .map(|variation| load(variation.drawn()))
                        .collect(),
                    placement_angle: unit.placement_angle.unwrap_or(Num::ZERO),
                })
            })
            .collect();
        Self { looks }
    }

    /// The models a unit of `unit_type` and `variation` draws: its variation's, or its look's own
    /// for a unit of none, of a look that does not vary, or of a place past the look's; `None`
    /// for a type of no look in the client data.
    pub(crate) fn look(
        &self,
        unit_type: UnitType,
        variation: Option<Variation>,
    ) -> Option<&[LookModel]> {
        Some(&self.drawn(unit_type, variation)?.models)
    }

    /// The top in meters of the highest model that a unit of `unit_type` and `variation` draws,
    /// as the import measured it; none for a type of no look, or of a look whose file gives no
    /// model's.
    pub(crate) fn top(&self, unit_type: UnitType, variation: Option<Variation>) -> Option<f32> {
        self.drawn(unit_type, variation)?.top
    }

    fn drawn(&self, unit_type: UnitType, variation: Option<Variation>) -> Option<&DrawnModels> {
        let look = self.looks.get(unit_type.index())?.as_ref()?;
        let place = variation.map(|variation| usize::from(variation.get()));
        Some(
            place
                .and_then(|place| look.variations.get(place))
                .unwrap_or(&look.own),
        )
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

    /// The models of `look`, a look of the package at `package`, each loading, its meshes kept in
    /// the main world as well as on the GPU, as the pointer casts its ray at their triangles
    /// (02-engine-core.md, A unit is picked by its drawn triangles).
    pub(crate) fn all(
        package: PackageIndex,
        look: DrawnLook<'_>,
        assets: &AssetServer,
    ) -> Vec<Self> {
        look.models
            .iter()
            .map(|model| Self {
                scene: assets
                    .load_builder()
                    .with_settings(|settings: &mut GltfLoaderSettings| {
                        settings.load_meshes = RenderAssetUsages::all();
                    })
                    .load(
                        GltfAssetLabel::Scene(0)
                            .from_asset(PackageAssets::asset_path(package, &model.model)),
                    ),
                nodes: LookNodes {
                    hide: model.hide.as_slice().into(),
                    turret: look.turret.map(Arc::from),
                },
                scale: model.scale.get(),
            })
            .collect()
    }
}

impl DrawnModels {
    fn load(package: PackageIndex, look: DrawnLook<'_>, assets: &AssetServer) -> Self {
        Self {
            models: LookModel::all(package, look, assets),
            top: look.top().map(ModelTop::get),
        }
    }
}

#[cfg(test)]
mod tests;
