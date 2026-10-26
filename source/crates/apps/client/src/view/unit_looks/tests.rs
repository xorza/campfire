use campfire_client_files::{ClientUnit, ClientUnits, LookVariation, ModelScale, NodeUpgrade};
use campfire_codec::Toml;
use campfire_declarations::ByIdBuilder;
use campfire_script::TagNames;
use campfire_units::{BodyForm, BodyShape, Layer, TypeOrigin};
use campfire_values::PackagePath;

use super::*;

#[test]
fn a_type_takes_the_models_of_its_key_in_its_packages_units() {
    let model = |path: &str, hide: &[&str]| ClientModel {
        model: PackagePath::parse(path).unwrap(),
        scale: ModelScale::ONE,
        hide: hide.iter().map(|&name| name.to_owned()).collect(),
        top: None,
        upgrades: Vec::new(),
    };
    let tall = |path: &str, top: f32| ClientModel {
        top: ModelTop::new(top),
        ..model(path, &[])
    };
    let units = |entries: Vec<(&str, Vec<ClientModel>)>| -> BTreeMap<_, _> {
        let units = entries
            .into_iter()
            .map(|(name, models)| (DeclaredName::new(name).unwrap(), ClientUnit::still(models)));
        units.collect()
    };
    let looks = |units| ClientUnits::new(units, BTreeMap::new(), BTreeMap::new()).unwrap();
    let mut mode = units(vec![
        (
            "tank",
            vec![
                model("client/models/tank.glb", &["tank.flash"]),
                tall("client/models/tread.glb", 1.5),
            ],
        ),
        ("sound", Vec::new()),
    ]);
    // The mode's tank is placed turned −135°.
    mode.get_mut("tank").unwrap().placement_angle = Some(Num::int(-135));
    let other = units(vec![("tank", vec![model("client/models/other.glb", &[])])]);
    let data = ClientLoad::of_looks([looks(mode), looks(other)]);
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
    let names = MatchNames::default();
    let types = LookTypes {
        origins: &origins,
        variations: None,
        names: &names,
    };
    let looks = UnitLooks::of(types, &data);
    let paths = |unit_type: UnitType| -> Option<Vec<(String, LookNodes)>> {
        let models = looks.look(unit_type, None)?;
        let path = |model: &LookModel| model.scene.to_string();
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
        turret_pitch: None,
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
    let tops = types.map(|unit_type| looks.draws(unit_type, None).and_then(LookDraws::top));
    assert_eq!(tops, [Some(1.5), None, None, None]);
}

#[test]
fn a_unit_of_a_type_of_variations_draws_its_variations_look_when_the_look_has_as_many() {
    let model = |name: &str, top: f32| ClientModel {
        model: PackagePath::parse(&format!("client/models/{name}.glb")).unwrap(),
        scale: ModelScale::ONE,
        hide: Vec::new(),
        top: ModelTop::new(top),
        upgrades: Vec::new(),
    };
    let variation = |name: &str, top: f32, turret: Option<&str>| LookVariation {
        draws: vec![LookDraw::still(vec![model(name, top)])],
        turret: turret.map(str::to_owned),
        turret_pitch: turret.map(|turret| format!("{turret}.elevation")),
    };
    let varied = |own: &str, variations: Vec<LookVariation>| ClientUnit {
        variations,
        ..ClientUnit::still(vec![model(own, 0.5)])
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
    let units = looks
        .into_iter()
        .map(|(name, look)| (DeclaredName::new(name).unwrap(), look));
    let data =
        ClientLoad::of_looks([
            ClientUnits::new(units.collect(), BTreeMap::new(), BTreeMap::new()).unwrap(),
        ]);
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
    let names = MatchNames::default();
    let types = LookTypes {
        origins: &origins,
        variations: Some(&variations),
        names: &names,
    };
    let looks = UnitLooks::of(types, &data);
    let drawn = |unit_type, variation: Option<u8>| {
        let variation = variation.map(Variation::new);
        let models = looks.look(unit_type, variation).unwrap();
        let path = models[0].scene.to_string();
        let name = path.trim_start_matches("package://0/client/models/");
        let name = name.trim_end_matches(".glb#Scene0");
        let top = looks.draws(unit_type, variation).and_then(LookDraws::top);
        (name.to_owned(), top)
    };
    let drawn_as = |name: &str, top: f32| (name.to_owned(), Some(top));
    // The technical's units draw their variations' models and tops, a unit of none, as a build
    // preview, its own, and a place past its look's its own.
    assert_eq!(drawn(technical, Some(0)), drawn_as("one", 1.0));
    assert_eq!(drawn(technical, Some(1)), drawn_as("two", 2.0));
    assert_eq!(drawn(technical, None), drawn_as("technical", 0.5));
    assert_eq!(drawn(technical, Some(2)), drawn_as("technical", 0.5));
    // The first variation turns and pitches its own nodes, and the second none.
    let turret = |place| {
        let models = looks.look(technical, Some(Variation::new(place))).unwrap();
        let nodes = &models[0].nodes;
        [&nodes.turret, &nodes.turret_pitch].map(|node| node.as_deref().map(str::to_owned))
    };
    let gun = ["gun", "gun.elevation"].map(|node| Some(node.to_owned()));
    assert_eq!([turret(0), turret(1)], [gun, [None, None]]);
    // The jeep's three looks of two variations, and the truck's two of none, are logged, and
    // each of their units draws its own; the tank's look, which does not vary, is its every
    // variation's.
    assert_eq!(drawn(jeep, Some(1)), drawn_as("jeep", 0.5));
    assert_eq!(drawn(truck, Some(0)), drawn_as("truck", 0.5));
    assert_eq!(drawn(tank, Some(1)), drawn_as("tank", 0.5));
}

#[test]
fn a_draw_draws_the_state_whose_set_best_matches_its_units_conditions() {
    let text = r#"
        [units.barracks]
        draws = [{ states = [
            { sets = [[]], models = [{ model = "day.glb" }] },
            { sets = [["night"]], models = [{ model = "night.glb" }] },
            { sets = [["snow"]], models = [{ model = "snow.glb" }] },
            { sets = [["damaged"], ["damaged", "night"]], models = [{ model = "hurt.glb" }] },
            { sets = [["moving", "rubble"]], models = [{ model = "walk.glb" }] },
            { sets = [["upgraded"]], models = [{ model = "upgraded.glb" }] },
            { sets = [["waiting"]], models = [{ model = "site.glb" }] },
            { sets = [["wip"]], models = [{ model = "frame.glb" }] },
        ], ignore = ["snow"] }]

        [conditions]
        damaged = { test = "life", above = "0.25", at_most = "0.5" }
        moving = { test = "moving", still_ms = 250 }
        night = { test = "map" }
        rubble = { test = "never" }
        snow = { test = "map" }
        upgraded = { test = "tag", tag = "weapon_upgrade" }
        waiting = { test = "site", stage = "waiting" }
        wip = { test = "site", stage = "started" }
        zap = { test = "attack", group = 1, phase = "reloading" }
        zing = { test = "attack", group = 1 }
        zombie = { test = "dead" }
    "#;
    let mut tags = TagNames::default();
    let upgrade = tags.names_mut().add("weapon_upgrade");
    let mut names = MatchNames::default();
    names.set_tags(tags);
    let origins: TypeOriginBook = [TypeOrigin {
        package: PackageIndex::MODE,
        name: "barracks".into(),
    }]
    .into_iter()
    .collect();
    let barracks = origins.iter().next().unwrap().0;
    let looks_on = |map: &[&str]| {
        let mut data = ClientLoad::of_looks([Toml::parse::<ClientUnits>(text).unwrap()]);
        data.map_conditions = map
            .iter()
            .map(|name| DeclaredName::new(name).unwrap())
            .collect();
        let types = LookTypes {
            origins: &origins,
            variations: None,
            names: &names,
        };
        UnitLooks::of(types, &data)
    };
    // The tested conditions' bits, by name: damaged 0, moving 1, upgraded 2, waiting 3, wip 4,
    // zap 5, zing 6, zombie 7; the map's and the one of no test are none.
    let [damaged, moving, upgraded, waiting, wip, zap, zing, zombie] =
        [1, 2, 4, 8, 16, 32, 64, 128];
    let drawn = |looks: &UnitLooks, held: u64| {
        let draw = &looks.draws(barracks, None).unwrap().draws()[0];
        let model = &draw.states()[draw.best(held)].shown.models[0];
        let path = model.scene.to_string();
        path.trim_start_matches("package://0/")
            .trim_end_matches(".glb#Scene0")
            .to_owned()
    };
    // On a night map of snow, whose snow the draw ignores: at rest the night state, of 1 in
    // common, past the default's none and the snow's, which lacks its ignored snow. Damaged:
    // the damaged night, of 2. Moving: the night still, of 1 and none lacked, past the walk's 1
    // and its condition of no test lacked. Upgraded: the night, first of a tie of 1 and none.
    let night = looks_on(&["night", "snow"]);
    let rest = &night.draws(barracks, None).unwrap().draws()[0];
    assert_eq!(rest.rest, 1);
    assert_eq!(drawn(&night, 0), "night");
    assert_eq!(drawn(&night, damaged), "hurt");
    assert_eq!(drawn(&night, moving), "night");
    assert_eq!(drawn(&night, upgraded), "night");
    // On a day map: the default at rest, the damaged state of 1, and the upgraded of 1.
    let day = looks_on(&[]);
    assert_eq!(day.draws(barracks, None).unwrap().draws()[0].rest, 0);
    assert_eq!(drawn(&day, 0), "day");
    assert_eq!(drawn(&day, damaged | moving), "hurt");
    assert_eq!(drawn(&day, upgraded), "upgraded");
    assert_eq!([drawn(&day, waiting), drawn(&day, wip)], ["site", "frame"]);

    // What a unit holds: its tag; its walk while it moved within 250 ms; its life above a
    // quarter and at most half of its most, 3 of 8; its site's stage.
    let tests = day.tests(barracks).unwrap();
    let still = UnitFacts {
        tags: TagSet::default(),
        walks: false,
        still_for: Duration::ZERO,
        life: Some([Num::int(8), Num::int(8)]),
        site: None,
        dead: false,
        attacks: 0,
    };
    assert_eq!(tests.held(still), 0);
    // Dead: zombie.
    let dead = UnitFacts {
        dead: true,
        ..still
    };
    assert_eq!(tests.held(dead), zombie);
    // A weapon of the second group reloads: zap, and zing, of any phase; one of the second
    // between shots, zing alone; one of the first, none.
    let attacking = |group, phase| UnitFacts {
        attacks: UnitFacts::attack_bit(group, phase).unwrap(),
        ..still
    };
    assert_eq!(
        [
            tests.held(attacking(1, AttackPhase::Reloading)),
            tests.held(attacking(0, AttackPhase::Reloading)),
            tests.held(attacking(1, AttackPhase::Between)),
        ],
        [zap | zing, 0, zing]
    );
    // Four bits a group: the second's reload is bit 7, the sixteenth's last bit 63, and the
    // second's at any phase bits 4 to 7; a seventeenth has none.
    assert_eq!(UnitFacts::attack_bits(1), Some(0b1111_0000));
    assert_eq!(UnitFacts::attack_bits(16), None);
    assert_eq!(
        UnitFacts::attack_bit(1, AttackPhase::Reloading),
        Some(1 << 7)
    );
    assert_eq!(
        UnitFacts::attack_bit(15, AttackPhase::Reloading),
        Some(1 << 63)
    );
    assert_eq!(UnitFacts::attack_bit(16, AttackPhase::PreAttack), None);
    let built = |stage| UnitFacts {
        site: Some(stage),
        ..still
    };
    assert_eq!(
        [SiteStage::Waiting, SiteStage::Started].map(|stage| tests.held(built(stage))),
        [waiting, wip]
    );
    let tagged = UnitFacts {
        tags: [upgrade].into_iter().collect(),
        ..still
    };
    assert_eq!(tests.held(tagged), upgraded);
    let walking = |ms| UnitFacts {
        walks: true,
        still_for: Duration::from_millis(ms),
        ..still
    };
    assert_eq!(
        [0, 250, 251].map(|ms| tests.held(walking(ms))),
        [moving, moving, 0]
    );
    let alive = |current| UnitFacts {
        life: Some([Num::int(current), Num::int(8)]),
        ..still
    };
    assert_eq!(
        [2, 3, 4, 5].map(|current| tests.held(alive(current))),
        [0, damaged, damaged, 0]
    );
    assert_eq!(
        tests.held(UnitFacts {
            life: None,
            ..still
        }),
        0
    );
    assert_eq!(tests.timed(), moving | zap | zing);
}

#[test]
fn a_models_upgrade_shows_and_hides_its_nodes_over_its_states_while_its_unit_holds_it() {
    // A tank of its flag hidden; its armor, tested by bit 2, shows the flag and hides the hatch;
    // its paint, a map's condition every unit holds, hides the hull; a condition of no test,
    // which none holds, shows the hatch.
    let upgrade = |condition: &str, show: &[&str], hide: &[&str]| NodeUpgrade {
        condition: DeclaredName::new(condition).unwrap(),
        show: show.iter().map(|&node| node.to_owned()).collect(),
        hide: hide.iter().map(|&node| node.to_owned()).collect(),
    };
    let tank = ClientModel {
        model: PackagePath::parse("tank.glb").unwrap(),
        scale: ModelScale::ONE,
        hide: vec!["flag".to_owned()],
        top: None,
        upgrades: vec![
            upgrade("armor", &["flag"], &["hatch"]),
            upgrade("paint", &[], &["hull"]),
            upgrade("rubble", &["hatch"], &[]),
        ],
    };
    let name = |name: &str| DeclaredName::new(name).unwrap();
    let known = BTreeMap::from([
        (name("armor"), Known::Tested(2)),
        (name("paint"), Known::Held),
        (name("rubble"), Known::Lacked),
    ]);
    let source = ModelSource {
        package: PackageIndex::MODE,
        turret: TurretNames::of(None, None),
        known: Some(&known),
    };
    let [model] = <[LookModel; 1]>::try_from(LookModel::all(source, &[tank])).unwrap();
    let hidden = |held| model.held_nodes(held).hide.to_vec();
    // Unarmored: the flag, then the hull; armored, bit 2: the flag shown, the hatch hidden, then
    // the hull, in the upgrades' order.
    assert_eq!(hidden(0), ["flag", "hull"]);
    assert_eq!(hidden(1 << 2), ["hatch", "hull"]);
    assert_eq!(hidden(1 << 3), ["flag", "hull"]);
    // Its upgrades test bit 2 alone; a source of no conditions holds none.
    assert_eq!(model.upgraded(), 1 << 2);
    let tank = LookModel::all(ModelSource::still(PackageIndex::MODE), &[model_file()]);
    assert_eq!(tank[0].upgraded(), 0);
}

/// A model file of an upgrade of a condition, as a look of no conditions reads it.
fn model_file() -> ClientModel {
    ClientModel {
        model: PackagePath::parse("tank.glb").unwrap(),
        scale: ModelScale::ONE,
        hide: Vec::new(),
        top: None,
        upgrades: vec![NodeUpgrade {
            condition: DeclaredName::new("armor").unwrap(),
            show: Vec::new(),
            hide: vec!["hatch".to_owned()],
        }],
    }
}
