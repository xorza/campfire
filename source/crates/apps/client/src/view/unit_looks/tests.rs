use campfire_client_files::{ClientModel, ClientUnit, ClientUnits, LookVariation, ModelScale};
use campfire_common::DeclaredName;
use campfire_declarations::ByIdBuilder;
use campfire_units::{BodyForm, BodyShape, Layer, TypeOrigin};
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
    let types = LookTypes {
        origins: &origins,
        variations: None,
    };
    let looks = UnitLooks::of(types, &data, app.world().resource::<AssetServer>());
    let paths = |unit_type: UnitType| -> Option<Vec<(String, LookNodes)>> {
        let models = looks.look(unit_type, None)?;
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
    let tops = types.map(|unit_type| looks.top(unit_type, None));
    assert_eq!(tops, [Some(1.5), None, None, None]);
}

#[test]
fn a_unit_of_a_type_of_variations_draws_its_variations_look_when_the_look_has_as_many() {
    let app = test_app::assets();
    let model = |name: &str, top: f32| ClientModel {
        model: PackagePath::parse(&format!("client/models/{name}.glb")).unwrap(),
        scale: ModelScale::ONE,
        hide: Vec::new(),
        top: ModelTop::new(top),
    };
    let variation = |name: &str, top: f32, turret: Option<&str>| LookVariation {
        models: vec![model(name, top)],
        variants: Vec::new(),
        turret: turret.map(str::to_owned),
    };
    let varied = |own: &str, variations: Vec<LookVariation>| ClientUnit {
        variations,
        ..ClientUnit::new(vec![model(own, 0.5)])
    };
    // The technical's look of two variations, of its type's two; the jeep's of three, of its
    // type's two; the truck's of two, of a type of none; the tank's of none, of a type of two.
    let two = || {
        vec![
            variation("one", 1.0, Some("gun")),
            variation("two", 2.0, None),
        ]
    };
    let mut three = two();
    three.push(variation("three", 3.0, None));
    let looks = [
        ("technical", varied("technical", two())),
        ("jeep", varied("jeep", three)),
        ("truck", varied("truck", two())),
        ("tank", varied("tank", Vec::new())),
    ];
    let names = looks.each_ref().map(|&(name, _)| name);
    let data = ClientLoad::of_looks([ClientUnits {
        units: looks
            .into_iter()
            .map(|(name, look)| (DeclaredName::new(name).unwrap(), look))
            .collect(),
        ..ClientUnits::default()
    }]);
    let origins: TypeOriginBook = names
        .map(|name| TypeOrigin {
            package: PackageIndex::MODE,
            name: name.into(),
        })
        .into_iter()
        .collect();
    let [technical, jeep, truck, tank] = [0, 1, 2, 3].map(|at| origins.iter().nth(at).unwrap().0);
    let form = BodyForm::new(BodyShape::circle(Num::ONE).unwrap(), Layer::FIRST);
    let mut variations = ByIdBuilder::default();
    for unit_type in [technical, jeep, tank] {
        variations.set(unit_type, Variations::from([form, form]));
    }
    let variations = variations.build();
    let types = LookTypes {
        origins: &origins,
        variations: Some(&variations),
    };
    let looks = UnitLooks::of(types, &data, app.world().resource::<AssetServer>());
    let drawn = |unit_type, variation: Option<u8>| {
        let variation = variation.map(Variation::new);
        let models = looks.look(unit_type, variation).unwrap();
        let path = models[0].scene.path().unwrap().to_string();
        let name = path.trim_start_matches("package://0/client/models/");
        let name = name.trim_end_matches(".glb#Scene0");
        (name.to_owned(), looks.top(unit_type, variation))
    };
    let drawn_as = |name: &str, top: f32| (name.to_owned(), Some(top));
    // The technical's units draw their variations' models and tops, a unit of none, as a build
    // preview, its own, and a place past its look's its own.
    assert_eq!(drawn(technical, Some(0)), drawn_as("one", 1.0));
    assert_eq!(drawn(technical, Some(1)), drawn_as("two", 2.0));
    assert_eq!(drawn(technical, None), drawn_as("technical", 0.5));
    assert_eq!(drawn(technical, Some(2)), drawn_as("technical", 0.5));
    // The first variation turns its own node, and the second none.
    let turret = |place| {
        let models = looks.look(technical, Some(Variation::new(place))).unwrap();
        models[0].nodes.turret.as_deref().map(str::to_owned)
    };
    assert_eq!([turret(0), turret(1)], [Some("gun".to_owned()), None]);
    // The jeep's three looks of two variations, and the truck's two of none, are logged, and
    // each of their units draws its own; the tank's look, which does not vary, is its every
    // variation's.
    assert_eq!(drawn(jeep, Some(1)), drawn_as("jeep", 0.5));
    assert_eq!(drawn(truck, Some(0)), drawn_as("truck", 0.5));
    assert_eq!(drawn(tank, Some(1)), drawn_as("tank", 0.5));
}
