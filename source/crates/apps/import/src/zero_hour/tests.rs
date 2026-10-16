use campfire_common::ContentHash;
use campfire_package_disk::PackageDir;
use campfire_sim::Capability;
use campfire_store::internals::Scratch;
use campfire_values::DeclaredName;
use serde::Deserialize;

use super::*;
use crate::texture::internals::ktx2;
use crate::zero_hour::armor_ini::internals::ARMORS;
use crate::zero_hour::big_archive::internals::big;
use crate::zero_hour::combat_import::WeaponDrop;
use crate::zero_hour::command_button_ini::internals::COMMAND_BUTTONS;
use crate::zero_hour::command_set_ini::internals::COMMAND_SETS;
use crate::zero_hour::error::{NameClash, VersionDifference};
use crate::zero_hour::game_data::GameData;
use crate::zero_hour::game_version::ArchiveHash;
use crate::zero_hour::install::internals::{fixture, packed_map};
use crate::zero_hour::locomotor_ini::internals::LOCOMOTORS;
use crate::zero_hour::map_file::internals::map;
use crate::zero_hour::model_import::{Converted, ModelImport};
use crate::zero_hour::object_ini::internals::{DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS};
use crate::zero_hour::player_template_ini::internals::PLAYER_TEMPLATES;
use crate::zero_hour::production_import::DropCause;
use crate::zero_hour::string_table::internals::strings;
use crate::zero_hour::weapon_ini::internals::WEAPONS;

/// The fixture install's version, as its archives hold their bytes now.
fn version_of(scratch: &Scratch) -> [GameVersion; 1] {
    let archives = ["A.big", "b.big", "c.BIG", "ZH_Generals/base.big"].map(|path| ArchiveHash {
        path,
        sha256: ContentHash::of(&scratch.read(format!("zh/{path}"))),
    });
    [GameVersion {
        name: "fixture",
        archives: Box::leak(Box::new(archives)),
    }]
}

#[test]
fn an_install_of_a_known_version_imports_its_maps_and_another_does_not() {
    let scratch = Scratch::new();
    fixture(&scratch);
    let known = version_of(&scratch);
    let zero_hour = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
    assert_eq!(zero_hour.version().name, "fixture");

    // The fixture map, in its folder `Fixture Map`, is `fixture_map`: its files are the ones
    // its import gives, and the unit types it places the package's.
    let imported = zero_hour.write(&scratch.path("one")).unwrap();
    let fingerprint = imported.fingerprint;
    // Its terrain's sand of 2 × 1 texels holds no tile, and its grass has no texture, so each
    // of the 3 × 2 cells the game draws is untextured.
    let named = MapName::parse("fixture_map").unwrap();
    assert_eq!(
        imported.maps,
        [(
            named.clone(),
            ImportedMap {
                untextured_cells: 6,
                dropped_waypoints: Vec::new(),
            }
        )]
        .into()
    );
    // Beside it, the rules package as the workspace holds it, which the imported mode, of
    // its manifest and mode data, depends on and loads with.
    assert_eq!(scratch.names("one"), ["game", "zero-hour"]);
    assert_eq!(
        scratch.names("one/game"),
        [
            "client",
            "data",
            "locale",
            "manifest.toml",
            "map",
            "package.index"
        ]
    );
    assert_eq!(
        scratch.read("one/zero-hour/package.index"),
        include_bytes!("../../../../../packages/zero-hour/package.index")
    );
    // The package's own reader takes its units file: the tank, the wall and the rocks the
    // map places, America's start objects, and the barracks and the ranger their command
    // sets make. Its units drive, by stats, orders and navigation, no faster than the tank's
    // 30.5 m a second.
    let mode = PackageDir::new(scratch.path("one/game"))
        .mode_packages(Some(&named))
        .unwrap();
    let types: Vec<String> = mode
        .content()
        .units
        .keys()
        .map(ToString::to_string)
        .collect();
    assert_eq!(
        types,
        [
            "americabarracks",
            "americacommandcenter",
            "americainfantryranger",
            "americatankcrusader",
            "americavehicledozer",
            "fortresswall",
            "rocks1"
        ]
    );
    // Its players start with America's command center and dozer, the dozer half the center's
    // sphere of 25 m, 12.5 m, along +z where a map has no rally, at the markers tagged
    // `start`, which the load checks a walker can stand on, and with the default money.
    let data: ModeView = Toml::parse(&scratch.read_text("one/game/data/mode.toml")).unwrap();
    assert_eq!(data.navigation.spawn_tags, ["start"]);
    assert_eq!(
        data.params,
        ParamsView {
            start_building: "americacommandcenter".to_owned(),
            start_units: vec!["americavehicledozer".to_owned()],
            start_offset: Num::int(25) / Num::int(2),
            start_money: 10000,
        }
    );
    assert_eq!(data.resources, ["money"]);
    assert_eq!(
        data.slots,
        ["weapon", "command"].map(|name| SlotView {
            name: name.to_owned()
        })
    );
    // The command sets' trains and builds: the center's dozer, the dozer's barracks, which
    // starts as it is ordered and its builder builds within 10 m, and the barracks' ranger,
    // which needs a barracks; each its object's cost, and the milliseconds of its frames, 5 s
    // 150 and 10 s 300, ⌊frames × 100 / 3⌋, which round back up to them.
    let action = |kind: &str, unit_type: &str, cost: u16, windup_ms: u64| ActionView {
        name: format!("action-{kind}-{unit_type}"),
        kind: kind.to_owned(),
        targeting: if kind == "train" { "none" } else { "point" }.to_owned(),
        unit_type: unit_type.to_owned(),
        cost: [("money".to_owned(), cost)].into(),
        windup_ms,
        requires: None,
        range: None,
        start: None,
        construct: None,
        start_life: None,
    };
    let barracks = ActionView {
        range: Some(Num::int(10)),
        start: Some("order".to_owned()),
        construct: Some("builder".to_owned()),
        // The barracks' site starts at 1 of its 1,200 life, 833,333.3… billionths rounded up.
        start_life: Some("0.000833334".to_owned()),
        ..action("build", "americabarracks", 600, 10000)
    };
    let ranger = ActionView {
        requires: Some(RequiresView {
            units: vec!["americabarracks".to_owned()],
        }),
        ..action("train", "americainfantryranger", 225, 5000)
    };
    // The weapons of the starting sets that hit one target at once: the rifle, the ranger's and
    // the crusader's, once, its 5 damage of small arms within 100 m, a shot each 100 ms, 3
    // frames, `ceil(100 × 0.03)`, in a clip of 3 that reloads in 700 ms, 21 frames.
    let rifle = WeaponView {
        kind: "attack".to_owned(),
        targeting: "enemies".to_owned(),
        range: Num::int(100),
        amount: Num::int(5),
        damage_kind: "small_arms".to_owned(),
        period_ms: 100,
        clip: Some(ClipView {
            shots: 3,
            reload_ms: 700,
        }),
    };
    assert_eq!(
        data.actions,
        [
            ("build_americabarracks".to_owned(), ActionOf::Made(barracks)),
            (
                "train_americainfantryranger".to_owned(),
                ActionOf::Made(ranger)
            ),
            (
                "train_americavehicledozer".to_owned(),
                ActionOf::Made(action("train", "americavehicledozer", 1000, 5000))
            ),
            ("weapon_rangerrifle".to_owned(), ActionOf::Weapon(rifle)),
        ]
        .into()
    );
    // The game's 38 damage types, in its order, as the mode's kinds; life the pool `life`;
    // every armor, of each kind it does not take whole: the soldier's crushing and sniping
    // twice, the building's every kind by `DEFAULT` at half and small arms at a quarter, but
    // `UNRESISTABLE` and `SUBDUAL_UNRESISTABLE`, which pass whole.
    assert_eq!(data.combat.damage_kinds.len(), 38);
    assert_eq!(
        [&data.combat.damage_kinds[0], &data.combat.damage_kinds[37]],
        ["explosion", "status"]
    );
    assert_eq!(data.combat.life, "life");
    let half = Num::int(1) / Num::int(2);
    assert_eq!(
        data.combat.armors["humanarmor"],
        [
            ("crush".to_owned(), Num::int(2)),
            ("sniper".to_owned(), Num::int(2))
        ]
        .into()
    );
    let building = &data.combat.armors["structurearmor"];
    assert_eq!(building.len(), 36);
    assert_eq!(building["explosion"], half);
    assert_eq!(building["small_arms"], Num::int(1) / Num::int(4));
    assert!(!building.contains_key("unresistable"));
    assert!(!building.contains_key("subdual_unresistable"));
    // Each producer holds its converted buttons in its set's order; a trainer its queue, 9
    // by default, and its exit's points, the game's `y` its `−z`.
    let units: UnitsView = Toml::parse(&scratch.read_text("one/game/data/units.toml")).unwrap();
    let command = |name: &str| {
        let slots = units.units[name].slots.as_ref();
        slots.map_or_default(|slots| slots.command.clone())
    };
    assert_eq!(
        command("americacommandcenter"),
        ["train_americavehicledozer"]
    );
    assert_eq!(command("americavehicledozer"), ["build_americabarracks"]);
    assert_eq!(command("americabarracks"), ["train_americainfantryranger"]);
    assert_eq!(command("americainfantryranger"), Vec::<String>::new());
    // Each type's weapons, of its starting set, in slot order, those dropped left out; its life,
    // its armor, and the command center's tag of victory.
    let weapons = |name: &str| {
        let slots = units.units[name].slots.as_ref();
        slots.map_or_default(|slots| slots.weapon.clone())
    };
    assert_eq!(weapons("americainfantryranger"), ["weapon_rangerrifle"]);
    assert_eq!(weapons("americatankcrusader"), ["weapon_rangerrifle"]);
    assert_eq!(weapons("americavehicledozer"), Vec::<String>::new());
    let life = |name: &str| {
        let unit = &units.units[name];
        let life = unit.stats.as_ref().and_then(|stats| stats.life.as_ref());
        (
            unit.pools.clone(),
            life.map(|life| life.base),
            unit.combat.as_ref(),
        )
    };
    let combat = |armor: Option<&str>| UnitCombatView {
        on_death: "despawn".to_owned(),
        armor: armor.map(str::to_owned),
    };
    let lives = Some(vec!["life".to_owned()]);
    assert_eq!(
        life("americainfantryranger"),
        (
            lives.clone(),
            Some(Num::int(180)),
            Some(&combat(Some("humanarmor")))
        )
    );
    assert_eq!(
        life("americacommandcenter"),
        (
            lives.clone(),
            Some(Num::int(5000)),
            Some(&combat(Some("structurearmor")))
        )
    );
    assert_eq!(
        life("americavehicledozer"),
        (lives, Some(Num::int(250)), Some(&combat(None)))
    );
    assert_eq!(life("fortresswall"), (None, None, None));
    assert_eq!(
        units.units["americacommandcenter"].tags,
        Some(vec!["counts_for_victory".to_owned()])
    );
    assert_eq!(units.units["americabarracks"].tags, None);
    // The weapons dropped, each by its cause, in the order the types are read: the ranger's
    // pistol of a random delay, the crusader's cannon of a shell and its railgun no file
    // defines; the flamer of the ranger's upgraded set is never read. The wall's highlander
    // body gives no life; the dozer starts at 200 of its 250, which the engine starts full.
    let dropped: Vec<(&str, WeaponDrop)> = imported
        .dropped_weapons
        .iter()
        .map(|dropped| (dropped.weapon.as_str(), dropped.cause))
        .collect();
    assert_eq!(
        dropped,
        [
            ("Pistol", WeaponDrop::RandomDelay),
            ("TankCannon", WeaponDrop::Projectile),
            ("Railgun", WeaponDrop::NoWeapon),
        ]
    );
    assert_eq!(imported.unliving, ["FortressWall"]);
    assert_eq!(imported.wounded, ["AmericaVehicleDozer"]);
    let point = |x: i64, z: i64| Some([Num::int(x), Num::int(z)]);
    assert_eq!(
        units.units["americacommandcenter"].production,
        Some(ProductionView {
            queue: 9,
            exit: point(-18, -35),
            rally: point(60, -35),
        })
    );
    assert_eq!(
        units.units["americabarracks"].production,
        Some(ProductionView {
            queue: 5,
            exit: point(0, 0),
            rally: point(55, 0),
        })
    );
    assert_eq!(units.units["americavehicledozer"].production, None);
    // The names, of the string table's labels, their hotkeys' markers dropped and `&&` an
    // `&`, braces escaped as Fluent reads them; the ranger's label the table lacks, so its
    // object's name.
    assert_eq!(
        scratch.read_text("one/game/locale/en.ftl"),
        "mode-name = Zero Hour\nteam-player-1 = Player 1\nteam-player-2 = Player 2\n\
         team-player-3 = Player 3\nteam-player-4 = Player 4\nteam-player-5 = Player 5\n\
         team-player-6 = Player 6\nteam-player-7 = Player 7\nteam-player-8 = Player 8\n\
         team-neutral = Neutral\n\
         action-build-americabarracks = Barracks & {\"{\"}Yard{\"}\"}\n\
         action-train-americainfantryranger = AmericaInfantryRanger\n\
         action-train-americavehicledozer = Dozer\n"
    );
    // The buttons dropped, each by its cause, in the order the sets are read: the center's,
    // the dozer's, then the barracks', which the dozer's set makes; the power plant, in both the
    // dozer's and the barracks' sets, once.
    let dropped: Vec<(&str, DropCause)> = imported
        .dropped_buttons
        .iter()
        .map(|dropped| (dropped.button.as_str(), dropped.cause))
        .collect();
    assert_eq!(
        dropped,
        [
            ("Command_ConstructAmericaPowerPlant", DropCause::Science),
            ("Command_ConstructAmericaFireBase", DropCause::NoBox),
            ("Command_ConstructAmericaInfantryRanger", DropCause::NoQueue),
            (
                "Command_ConstructAmericaInfantryPathfinder",
                DropCause::EitherOr
            ),
            ("Command_ConstructFortressWall", DropCause::NoWalker),
            ("Command_ConstructGhost", DropCause::NoObject),
        ]
    );
    let manifest = mode.manifest();
    assert_eq!(manifest.max_move_speed.get(), Num::int(61) / Num::int(2));
    let drives = [
        Capability::Stats,
        Capability::Orders,
        Capability::Navigation,
        Capability::Abilities,
        Capability::Production,
        Capability::Combat,
    ];
    // Eight players, each a team of one slot, and the neutral team of none.
    assert_eq!(manifest.teams.len(), 9);
    assert!(manifest.teams[..8].iter().all(|team| team.slots == 1));
    assert!(
        drives
            .iter()
            .all(|&capability| manifest.capabilities.contains(capability))
    );
    assert_eq!(scratch.names("one/game/map"), ["fixture_map"]);
    assert_eq!(
        scratch.names("one/game/map/fixture_map"),
        ["heights.bin", "map.toml"]
    );
    assert_eq!(
        scratch.names("one/game/client/maps/fixture_map"),
        [
            "conditions.toml",
            "lights.toml",
            "objects.toml",
            "terrain.bin"
        ]
    );
    // The class whose type's texture the install holds names it, the sign's TGA of 2 × 1
    // texels; the other names none.
    let mut file = MapFile::read(&map(8)).unwrap();
    file.terrain.classes[0].texture = Some(ClassTexture {
        path: PackagePath::parse("client/textures/art/terrain/tsand.ktx2").unwrap(),
        size: UVec2::new(2, 1),
    });
    let mut ini = ObjectIni::default();
    for objects in [DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS] {
        ini.read(objects.as_bytes()).unwrap();
    }
    let mut locomotors = LocomotorIni::default();
    locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    let looks = Toml::parse(&scratch.read_text("one/game/client/units.toml")).unwrap();
    let mut names = ImportNames::default();
    let sources = MapSources {
        ini: &ini,
        locomotors: &locomotors,
        names: &mut names,
        looks: &looks,
        lights: 3,
        game: GameData {
            adjust_cliff_textures: true,
            three_way_blends: false,
        },
    };
    let expected = MapImport::new(&file, sources).unwrap();
    assert_eq!(
        scratch.read_text("one/game/map/fixture_map/map.toml"),
        expected.map
    );
    assert_eq!(
        scratch.read("one/game/map/fixture_map/heights.bin"),
        expected.heights
    );
    assert_eq!(
        scratch.read("one/game/client/maps/fixture_map/terrain.bin"),
        expected.terrain
    );
    assert_eq!(
        scratch.read_text("one/game/client/maps/fixture_map/lights.toml"),
        expected.lights
    );
    assert_eq!(
        scratch.read_text("one/game/client/maps/fixture_map/objects.toml"),
        expected.objects
    );
    assert_eq!(
        scratch.read_text("one/game/client/maps/fixture_map/conditions.toml"),
        expected.conditions
    );
    // The units file holds the map's types and the start's, which the start's own reading
    // of the templates adds, those the command sets make, as the import adds them, and their
    // weapons.
    let mut templates = PlayerTemplateIni::default();
    templates.read(PLAYER_TEMPLATES.as_bytes()).unwrap();
    SkirmishStart::of(&templates, &ini, &locomotors, &mut names).unwrap();
    let mut sets = CommandSetIni::default();
    sets.read(COMMAND_SETS.as_bytes()).unwrap();
    let mut buttons = CommandButtonIni::default();
    buttons.read(COMMAND_BUTTONS.as_bytes()).unwrap();
    let strings = StringTable::read(&strings()).unwrap();
    let sources = CommandSources {
        ini: &ini,
        sets: &sets,
        buttons: &buttons,
        locomotors: &locomotors,
        strings: &strings,
    };
    ProductionImport::of(sources, &mut names);
    let mut weapons = WeaponIni::default();
    weapons.read(WEAPONS.as_bytes()).unwrap();
    let mut armors = ArmorIni::default();
    armors.read(ARMORS.as_bytes()).unwrap();
    let combat = CombatSources {
        ini: &ini,
        weapons: &weapons,
        armors: &armors,
    };
    CombatImport::of(combat, &mut names).unwrap();
    let units = names.units_file();
    assert_eq!(scratch.read_text("one/game/data/units.toml"), units.text);
    let camera = Toml::parse::<CameraFile>(&scratch.read_text("one/game/client/camera.toml"));
    assert_eq!(camera.unwrap().field_of_view, 50.0);
    // Each texture once, at its path in the archives, as its conversion gives it.
    let [rock, sign] = ktx2();
    assert_eq!(
        scratch.names("one/game/client/textures/art/textures"),
        ["rock.ktx2", "sign.ktx2"]
    );
    assert_eq!(
        scratch.read("one/game/client/textures/art/textures/rock.ktx2"),
        rock
    );
    assert_eq!(
        scratch.read("one/game/client/textures/art/textures/sign.ktx2"),
        sign
    );
    assert_eq!(
        scratch.read("one/game/client/textures/art/terrain/tsand.ktx2"),
        sign
    );
    // Two imports write the same bytes, and the package reads as the fingerprint names it.
    assert_eq!(
        zero_hour.write(&scratch.path("two")).unwrap().fingerprint,
        fingerprint
    );
    assert_eq!(
        scratch.read("two/game/package.index"),
        scratch.read("one/game/package.index")
    );
    assert_eq!(
        PackageDir::new(scratch.path("one/game"))
            .files()
            .unwrap()
            .fingerprint(),
        fingerprint
    );
}

#[test]
fn an_install_of_two_maps_of_one_name_or_of_an_unknown_version_does_not_import() {
    let scratch = Scratch::new();
    fixture(&scratch);
    // A second folder whose name gives `fixture_map` is refused.
    scratch.write(
        "zh/c.BIG",
        big(&[
            ("c.ini", b"c"),
            ("Maps\\Fixture_Map\\Fixture_Map.map", &packed_map()),
        ]),
    );
    let known = version_of(&scratch);
    let clash = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
    assert!(matches!(
        clash.write(&scratch.path("three")),
        Err(ZeroHourError::MapNameClash { first, second })
            if first == "maps\\fixture map\\fixture map.map"
                && second == "maps\\fixture_map\\fixture_map.map"
    ));

    // An archive of other bytes is no version the importer knows.
    scratch.write("zh/c.BIG", b"BIGF");
    assert!(matches!(
        ZeroHour::open(&scratch.path("zh"), &known),
        Err(ZeroHourError::Archive { .. })
    ));
    scratch.write("zh/c.BIG", big(&[("c.ini", b"C")]));
    assert!(matches!(
        ZeroHour::open(&scratch.path("zh"), &known),
        Err(ZeroHourError::UnknownVersion {
            archive,
            difference: VersionDifference::OtherBytes,
        }) if archive == "c.big"
    ));
}

#[test]
fn an_import_writes_each_model_once_and_each_object_s_models() {
    let scratch = Scratch::new();
    fixture(&scratch);
    let known = version_of(&scratch);
    let zero_hour = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
    let imported = zero_hour.write(&scratch.path("one")).unwrap();
    // Each model the objects name once, as its conversion gives it; its material file; and
    // each object's models with the nodes its default look hides. The tank's turret is hidden
    // with the flash on the bone below it, which its muzzle flash hides as well; the crate's
    // box and the marker's `NULL` draw nothing; the tank's aggregate, the emitter, the absent
    // model and the tree's, which the fixture install lacks, are skipped, in the order of their
    // objects' names.
    assert_eq!(
        imported.skipped_models,
        [
            SkippedModel {
                name: "tank.smoke".to_owned(),
                reason: SkipReason::Aggregate,
            },
            SkippedModel {
                name: "absent".to_owned(),
                reason: SkipReason::Missing,
            },
            SkippedModel {
                name: "spray".to_owned(),
                reason: SkipReason::Emitter,
            },
            SkippedModel {
                name: "ptoak01".to_owned(),
                reason: SkipReason::Missing,
            },
        ]
    );
    // The flash's texture is in no archive: its triangle is untextured, and the import says so.
    assert_eq!(imported.missing_textures, ["gone.tga"]);
    assert_eq!(
        scratch.names("one/game/client/models"),
        ["rock01.glb", "tank.glb"]
    );
    let install = Install::open(&scratch.path("zh")).unwrap();
    let mut assets = ModelAssets::new(&install).unwrap();
    let Ok(Converted::Model(tank)) = ModelImport::convert("tank", &mut assets) else {
        panic!("the tank converts");
    };
    assert_eq!(scratch.names("one/game/client/materials"), ["tank_3.toml"]);
    assert_eq!(
        scratch.read_text("one/game/client/materials/tank_3.toml"),
        Toml::write(&tank.materials[0].file)
    );
    assert_eq!(
        scratch.read("one/game/client/models/tank.glb"),
        tank.glb.encode()
    );
    let scaled = |name: &str, scale: f32, hide: &[&str]| ClientModel {
        model: PackagePath::parse(&format!("client/models/{name}.glb")).unwrap(),
        scale: ModelScale::new(scale).unwrap(),
        hide: hide.iter().map(|&node| node.to_owned()).collect(),
    };
    let model = |name: &str, hide: &[&str]| scaled(name, 1.0, hide);
    let unit = ClientUnit::new;
    let looks = |looks: Vec<(&str, ClientUnit)>| {
        looks
            .into_iter()
            .map(|(name, unit)| (DeclaredName::new(name).unwrap(), unit))
            .collect()
    };
    // The tank's night state shows its turret, and with it the flash below: a night map, and a
    // night of snow, draw it whole; a snowy map its default, which the variant names too, so
    // the client's best match finds each.
    let hidden = || vec![model("tank", &["tank.turret", "tank.flash"])];
    let variant = |conditions: &[&str], models| LookVariant {
        conditions: conditions
            .iter()
            .map(|condition| DeclaredName::new(condition).unwrap())
            .collect(),
        models,
    };
    let tank = || ClientUnit {
        models: hidden(),
        variants: vec![
            variant(&["night"], vec![model("tank", &[])]),
            variant(&["snow"], hidden()),
            variant(&["night", "snow"], vec![model("tank", &[])]),
        ],
        placement_angle: None,
    };
    // The rock and its reskin at the rock's scale, which the reskin cannot set; the tree's
    // look as a unit of both its models, and as the client's of its tree draw module's; the
    // fluff's as a unit of its hidden turret, with the flash below it, and as the client's of
    // none. An object of no draw module, of a model of `NONE`, of `NULL` or of a box draws
    // nothing, so its look is empty; one of a model the package lacks has none. The command
    // center's look keeps the angle its build's preview places it at.
    let rock = || unit(vec![scaled("rock01", 1.5, &[])]);
    let empty = || unit(Vec::new());
    let expected = ClientUnits {
        units: looks(vec![
            ("americabarracks", empty()),
            (
                "americacommandcenter",
                ClientUnit {
                    placement_angle: Some(Num::int(-135)),
                    ..empty()
                },
            ),
            ("americafirebase", empty()),
            ("americainfantrypathfinder", empty()),
            ("americainfantryranger", empty()),
            ("americapowerplant", empty()),
            ("americatankcrusader", empty()),
            ("americavehicledozer", empty()),
            ("bush", unit(hidden())),
            ("crate", empty()),
            ("fortresswall", empty()),
            ("marker", empty()),
            ("oak", unit(vec![model("tank", &[]), model("rock01", &[])])),
            ("rock", rock()),
            ("rockred", rock()),
            ("rocks1", empty()),
            ("stump", empty()),
            ("tank", tank()),
            ("tankcamo", tank()),
            ("tankred", unit(vec![model("tank", &[])])),
        ]),
        objects: looks(vec![
            ("bush", unit(vec![model("tank", &[])])),
            ("oak", unit(vec![model("rock01", &[])])),
            ("rock", rock()),
            ("rockred", rock()),
        ]),
    };
    assert_eq!(
        Toml::parse::<ClientUnits>(&scratch.read_text("one/game/client/units.toml")).unwrap(),
        expected
    );

    // Two objects whose names give one unit type, each with a model, are refused.
    let clash = "Object Rock!\nDraw = W3DPropDraw M\nModelName = Rock01\nEnd\nEnd\nObject Rock?\nDraw = W3DPropDraw M\nModelName = Rock01\nEnd\nEnd\n";
    scratch.write(
        "zh/c.BIG",
        big(&[
            ("c.ini", b"c"),
            ("Data\\INI\\Object\\Clash.ini", clash.as_bytes()),
        ]),
    );
    let known = version_of(&scratch);
    let clash = ZeroHour::open(&scratch.path("zh"), &known).unwrap();
    assert!(matches!(
        clash.write(&scratch.path("two")),
        Err(ZeroHourError::NameClash(NameClash { first, second }))
            if first == "Rock!" && second == "Rock?"
    ));
}

/// What the test reads of the mode data the import writes.
#[derive(Debug, Deserialize)]
struct ModeView {
    resources: Vec<String>,
    navigation: NavigationView,
    slots: Vec<SlotView>,
    params: ParamsView,
    combat: CombatView,
    actions: BTreeMap<String, ActionOf>,
}

#[derive(Debug, Deserialize)]
struct CombatView {
    damage_kinds: Vec<String>,
    life: String,
    armors: BTreeMap<String, BTreeMap<String, Num>>,
}

/// An action of the mode data: a weapon, or a train or a build.
#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
enum ActionOf {
    Weapon(WeaponView),
    Made(ActionView),
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct WeaponView {
    kind: String,
    targeting: String,
    range: Num,
    amount: Num,
    damage_kind: String,
    period_ms: u64,
    clip: Option<ClipView>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct ClipView {
    shots: u16,
    reload_ms: u64,
}

#[derive(Debug, Deserialize)]
struct NavigationView {
    spawn_tags: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct SlotView {
    name: String,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct ParamsView {
    start_building: String,
    start_units: Vec<String>,
    start_offset: Num,
    start_money: u32,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct ActionView {
    name: String,
    kind: String,
    targeting: String,
    unit_type: String,
    #[serde(default)]
    cost: BTreeMap<String, u16>,
    windup_ms: u64,
    requires: Option<RequiresView>,
    range: Option<Num>,
    start: Option<String>,
    construct: Option<String>,
    start_life: Option<String>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct RequiresView {
    units: Vec<String>,
}

/// What the test reads of the units file the import writes.
#[derive(Debug, Deserialize)]
struct UnitsView {
    units: BTreeMap<String, UnitView>,
}

#[derive(Debug, Deserialize)]
struct UnitView {
    pools: Option<Vec<String>>,
    tags: Option<Vec<String>>,
    stats: Option<StatsView>,
    combat: Option<UnitCombatView>,
    slots: Option<SlotsView>,
    production: Option<ProductionView>,
}

#[derive(Debug, Deserialize)]
struct StatsView {
    life: Option<BaseView>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct BaseView {
    base: Num,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct UnitCombatView {
    on_death: String,
    armor: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SlotsView {
    #[serde(default)]
    weapon: Vec<String>,
    #[serde(default)]
    command: Vec<String>,
}

#[derive(Debug, PartialEq, Eq, Deserialize)]
struct ProductionView {
    queue: u8,
    exit: Option<[Num; 2]>,
    rally: Option<[Num; 2]>,
}
