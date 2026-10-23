use campfire_client_files::{ClientModel, ClientUnits, ModelScale};
use campfire_common::DeclaredName;
use campfire_units::TypeOrigin;
use campfire_values::PackagePath;

use super::*;
use crate::test_app;

#[test]
fn a_type_takes_the_models_of_its_key_in_its_packages_units() {
    let app = test_app::assets();
    let model = |path: &str, hide: &[&str]| ClientModel {
        model: PackagePath::parse(path).unwrap(),
        scale: ModelScale::ONE,
        hide: hide.iter().map(|&name| name.to_owned()).collect(),
        top: None,
    };
    let tall = |path: &str, top: f32| ClientModel {
        top: ModelTop::new(top),
        ..model(path, &[])
    };
    let units = |entries: Vec<(&str, Vec<ClientModel>)>| ClientUnits {
        units: entries
            .into_iter()
            .map(|(name, models)| (DeclaredName::new(name).unwrap(), ClientUnit::new(models)))
            .collect(),
        ..ClientUnits::default()
    };
    let mut data = ClientLoad::of_looks([
        units(vec![
            (
                "tank",
                vec![
                    model("client/models/tank.glb", &["tank.flash"]),
                    tall("client/models/tread.glb", 1.5),
                ],
            ),
            ("sound", Vec::new()),
        ]),
        units(vec![("tank", vec![model("client/models/other.glb", &[])])]),
    ]);
    // The mode's tank is placed turned −135°.
    let tank = DeclaredName::new("tank").unwrap();
    data.packages[0]
        .units
        .units
        .get_mut(&tank)
        .unwrap()
        .placement_angle = Some(Num::int(-135));
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
    let paths = |unit_type: UnitType| -> Option<Vec<(String, LookNodes)>> {
        let models = looks.look(unit_type)?;
        let path = |model: &LookModel| model.scene.path().unwrap().to_string();
        Some(
            models
                .iter()
                .map(|model| (path(model), model.nodes.clone()))
                .collect(),
        )
    };
    let types: [UnitType; 4] = origins
        .iter()
        .map(|(unit_type, _)| unit_type)
        .collect::<Vec<_>>()
        .try_into()
        .unwrap();
    let look_nodes = |hide: &[&str]| LookNodes {
        hide: hide.iter().map(|&name| name.to_owned()).collect(),
        turret: None,
    };
    assert_eq!(
        paths(types[0]).unwrap(),
        [
            (
                "package://0/client/models/tank.glb#Scene0".to_owned(),
                look_nodes(&["tank.flash"])
            ),
            (
                "package://0/client/models/tread.glb#Scene0".to_owned(),
                look_nodes(&[])
            ),
        ]
    );
    assert_eq!(
        paths(types[1]).unwrap(),
        [(
            "package://1/client/models/other.glb#Scene0".to_owned(),
            look_nodes(&[])
        )]
    );
    assert_eq!(paths(types[2]), None);
    assert_eq!(paths(types[3]), Some(Vec::new()));
    // The tank's angle; 0 for package 1's tank, which gives none, and for the type of no look.
    let angles = types.map(|unit_type| looks.placement_angle(unit_type));
    assert_eq!(angles, [-135, 0, 0, 0].map(Num::int));
    // The mode's tank's top, its tread's 1.5 m, of a hull whose file gives none; none for
    // package 1's tank, whose file gives none, the type of no look, and the look of no models.
    let tops = types.map(|unit_type| looks.top(unit_type));
    assert_eq!(tops, [Some(1.5), None, None, None]);
}
