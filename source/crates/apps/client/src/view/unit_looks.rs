use std::sync::Arc;

use bevy::asset::{AssetServer, Handle};
use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use bevy::gltf::GltfAssetLabel;
use bevy::world_serialization::WorldAsset;
use campfire_client_files::ClientUnit;
use campfire_math::Num;
use campfire_units::TypeOriginBook;
use campfire_values::{NameId, PackageIndex, UnitType};

use crate::view::client_data::ClientData;
use crate::view::package_source::PackageSource;

/// Each unit type's look, as its package's client data gives it, by type: none for a type of no
/// look, drawn as the engine's own shapes are.
#[derive(Resource, Debug, Default)]
pub(crate) struct UnitLooks {
    looks: Vec<Option<TypeLook>>,
}

/// A type's look: its models, none for a look drawn as nothing, and the angle in degrees a build
/// preview places it at.
#[derive(Debug)]
struct TypeLook {
    models: Vec<LookModel>,
    placement_angle: Num,
}

/// A model a look is drawn with: its glTF's first scene, its parts, and the factor it is drawn
/// at.
#[derive(Debug, Clone)]
pub(crate) struct LookModel {
    pub(crate) scene: Handle<WorldAsset>,
    pub(crate) parts: ModelParts,
    pub(crate) scale: f32,
}

/// What a drawn model needs once its scene is spawned: its package's place, whose material files
/// its materials take, the names of the nodes its default look hides, and the name of the node
/// its look turns by its unit's turret angle, if it has one.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelParts {
    pub(crate) package: PackageIndex,
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
                data.units_of(package)
                    .units
                    .get(&*origin.name)
                    .map(|unit| TypeLook {
                        models: LookModel::all(package, unit, assets),
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

    /// The angle in degrees a build preview places `unit_type` at: its look's, 0 for a look that
    /// gives none or a type of no look.
    pub(crate) fn placement_angle(&self, unit_type: UnitType) -> Num {
        let look = self.looks.get(unit_type.index()).and_then(Option::as_ref);
        look.map_or(Num::ZERO, |look| look.placement_angle)
    }
}

impl LookModel {
    /// The models of `look`, a look of the package at `package`, each loading.
    pub(crate) fn all(package: PackageIndex, look: &ClientUnit, assets: &AssetServer) -> Vec<Self> {
        look.models
            .iter()
            .map(|model| Self {
                scene: assets.load(
                    GltfAssetLabel::Scene(0)
                        .from_asset(PackageSource::asset_path(package, &model.model)),
                ),
                parts: ModelParts {
                    package,
                    hide: model.hide.as_slice().into(),
                    turret: look.turret.as_deref().map(Arc::from),
                },
                scale: model.scale.get(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use bevy::app::{App, TaskPoolPlugin};
    use bevy::asset::{AssetApp, AssetPlugin};
    use campfire_client_files::{ClientModel, ClientUnits, ModelScale};
    use campfire_common::DeclaredName;
    use campfire_units::TypeOrigin;
    use campfire_values::PackagePath;

    use super::*;

    #[test]
    fn a_type_takes_the_models_of_its_key_in_its_packages_units() {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
        app.init_asset::<WorldAsset>();
        let model = |path: &str, hide: &[&str]| ClientModel {
            model: PackagePath::parse(path).unwrap(),
            scale: ModelScale::ONE,
            hide: hide.iter().map(|&name| name.to_owned()).collect(),
        };
        let units = |entries: Vec<(&str, Vec<ClientModel>)>| ClientUnits {
            units: entries
                .into_iter()
                .map(|(name, models)| (DeclaredName::new(name).unwrap(), ClientUnit::new(models)))
                .collect(),
            ..ClientUnits::default()
        };
        let mut data = ClientData {
            units: vec![
                units(vec![
                    (
                        "tank",
                        vec![
                            model("client/models/tank.glb", &["tank.flash"]),
                            model("client/models/tread.glb", &[]),
                        ],
                    ),
                    ("sound", Vec::new()),
                ]),
                units(vec![("tank", vec![model("client/models/other.glb", &[])])]),
            ],
            ..ClientData::default()
        };
        // The mode's tank is placed turned −135°.
        let tank = DeclaredName::new("tank").unwrap();
        data.units[0].units.get_mut(&tank).unwrap().placement_angle = Some(Num::int(-135));
        // The mode's tank, package 1's tank, a type no package's units name, and the mode's
        // sound, of a look of no models.
        let origin = |package, name: &str| TypeOrigin {
            package,
            name: name.into(),
        };
        let origins: TypeOriginBook = [
            origin(PackageIndex::MODE, "tank"),
            origin(PackageIndex::new(1), "tank"),
            origin(PackageIndex::MODE, "tower"),
            origin(PackageIndex::MODE, "sound"),
        ]
        .into_iter()
        .collect();
        let looks = UnitLooks::of(&origins, &data, app.world().resource::<AssetServer>());
        let paths = |unit_type: UnitType| -> Option<Vec<(String, ModelParts)>> {
            let models = looks.look(unit_type)?;
            let path = |model: &LookModel| model.scene.path().unwrap().to_string();
            Some(
                models
                    .iter()
                    .map(|model| (path(model), model.parts.clone()))
                    .collect(),
            )
        };
        let types: [UnitType; 4] = origins
            .iter()
            .map(|(unit_type, _)| unit_type)
            .collect::<Vec<_>>()
            .try_into()
            .unwrap();
        let parts = |package: PackageIndex, hide: &[&str]| ModelParts {
            package,
            hide: hide.iter().map(|&name| name.to_owned()).collect(),
            turret: None,
        };
        assert_eq!(
            paths(types[0]).unwrap(),
            [
                (
                    "package://0/client/models/tank.glb#Scene0".to_owned(),
                    parts(PackageIndex::MODE, &["tank.flash"])
                ),
                (
                    "package://0/client/models/tread.glb#Scene0".to_owned(),
                    parts(PackageIndex::MODE, &[])
                ),
            ]
        );
        assert_eq!(
            paths(types[1]).unwrap(),
            [(
                "package://1/client/models/other.glb#Scene0".to_owned(),
                parts(PackageIndex::new(1), &[])
            )]
        );
        assert_eq!(paths(types[2]), None);
        assert_eq!(paths(types[3]), Some(Vec::new()));
        // The tank's angle; 0 for package 1's tank, which gives none, and for the type of no look.
        let angles = types.map(|unit_type| looks.placement_angle(unit_type));
        assert_eq!(angles, [-135, 0, 0, 0].map(Num::int));
    }
}
