use bevy::asset::{AssetServer, Handle};
use bevy::ecs::component::Component;
use bevy::ecs::resource::Resource;
use bevy::gltf::GltfAssetLabel;
use bevy::world_serialization::WorldAsset;
use campfire_capabilities::{TypeOrigins, UnitType};

use crate::view::client_data::ClientData;
use crate::view::package_source::PackageSource;

/// Each unit type's models, as its package's client data lists them, by type: none for a type
/// drawn as the engine's own shapes are.
#[derive(Resource, Debug, Default)]
pub(crate) struct UnitLooks {
    looks: Vec<Vec<LookModel>>,
}

/// A model a unit type is drawn with: its glTF's first scene, and its parts.
#[derive(Debug, Clone)]
pub(crate) struct LookModel {
    pub(crate) scene: Handle<WorldAsset>,
    pub(crate) parts: ModelParts,
}

/// What a drawn model needs once its scene is spawned: its package's place, whose material files
/// its materials take, and the names of the nodes its default look hides.
#[derive(Component, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModelParts {
    pub(crate) package: u16,
    pub(crate) hide: Vec<String>,
}

impl UnitLooks {
    /// The looks of every type of `origins`: the models of the type's key in its package's units.
    pub(crate) fn of(origins: &TypeOrigins, data: &ClientData, assets: &AssetServer) -> UnitLooks {
        let looks =
            origins
                .iter()
                .map(|(_, origin)| {
                    let units = &data.units[usize::from(origin.package)];
                    let Some(unit) = units.units.get(&*origin.name) else {
                        return Vec::new();
                    };
                    unit.models
                        .iter()
                        .map(|model| LookModel {
                            scene: assets.load(GltfAssetLabel::Scene(0).from_asset(
                                PackageSource::asset_path(origin.package, &model.model),
                            )),
                            parts: ModelParts {
                                package: origin.package,
                                hide: model.hide.clone(),
                            },
                        })
                        .collect()
                })
                .collect();
        UnitLooks { looks }
    }

    /// The models of `unit_type`; none for a type the client data does not draw.
    pub(crate) fn models(&self, unit_type: UnitType) -> &[LookModel] {
        self.looks.get(unit_type.index()).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use bevy::app::{App, TaskPoolPlugin};
    use bevy::asset::{AssetApp, AssetPlugin};
    use campfire_capabilities::{PackagePath, TypeOrigin};
    use campfire_package::{ClientModel, ClientUnit, ClientUnits};

    use super::*;

    #[test]
    fn a_type_takes_the_models_of_its_key_in_its_packages_units() {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()));
        app.init_asset::<WorldAsset>();
        let model = |path: &str, hide: &[&str]| ClientModel {
            model: PackagePath::parse(path).unwrap(),
            hide: hide.iter().map(|&name| name.to_owned()).collect(),
        };
        let units = |entries: Vec<(&str, Vec<ClientModel>)>| ClientUnits {
            units: entries
                .into_iter()
                .map(|(name, models)| {
                    (
                        campfire_capabilities::DeclaredName::new(name).unwrap(),
                        ClientUnit { models },
                    )
                })
                .collect(),
        };
        let data = ClientData {
            units: vec![
                units(vec![(
                    "tank",
                    vec![
                        model("client/models/tank.glb", &["tank.flash"]),
                        model("client/models/tread.glb", &[]),
                    ],
                )]),
                units(vec![("tank", vec![model("client/models/other.glb", &[])])]),
            ],
            ..ClientData::default()
        };
        // The mode's tank, package 1's tank, and a type no package's units name.
        let origin = |package, name: &str| TypeOrigin {
            package,
            name: name.into(),
        };
        let origins: TypeOrigins = [origin(0, "tank"), origin(1, "tank"), origin(0, "tower")]
            .into_iter()
            .collect();
        let looks = UnitLooks::of(&origins, &data, app.world().resource::<AssetServer>());
        let paths = |unit_type: UnitType| -> Vec<(String, ModelParts)> {
            looks
                .models(unit_type)
                .iter()
                .map(|model| (model.scene.path().unwrap().to_string(), model.parts.clone()))
                .collect()
        };
        let types: Vec<UnitType> = origins.iter().map(|(unit_type, _)| unit_type).collect();
        let parts = |package, hide: &[&str]| ModelParts {
            package,
            hide: hide.iter().map(|&name| name.to_owned()).collect(),
        };
        assert_eq!(
            paths(types[0]),
            [
                (
                    "package://0/client/models/tank.glb#Scene0".to_owned(),
                    parts(0, &["tank.flash"])
                ),
                (
                    "package://0/client/models/tread.glb#Scene0".to_owned(),
                    parts(0, &[])
                ),
            ]
        );
        assert_eq!(
            paths(types[1]),
            [(
                "package://1/client/models/other.glb#Scene0".to_owned(),
                parts(1, &[])
            )]
        );
        assert!(paths(types[2]).is_empty());
    }
}
