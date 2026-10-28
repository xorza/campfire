use std::num::NonZeroU32;
use std::sync::Arc;

use campfire_actions::{
    ActionData, ActionKeys, ActionRange, ActionsData, RangeField, SlotKinds, Targeting,
    TypeSlotsData,
};
use campfire_client_files::{
    ActionLook, AnimationMode, BarrelFlash, ClientModel, ClientUnit, ClipRounds, ConditionTest,
    FireBarrel, FireBone, Key, LookDraw, LookShown, LookState, LookTransition, LookVariation,
    ModelScale, ModelTop, NodeUpgrade, ObjectLook, PanelButton, PanelCommand, PanelPlace, Press,
    StateAnimation, StateClip, StatePlay,
};
use campfire_combat::{
    ArmorShare, AttackData, BonusData, ClipData, ClipRefill, CombatData, ModeCombatData, OnDeath,
    PitchData, ScatterData, SplashData, TurretData, UnitOffset, WeaponValue,
};
use campfire_math::Rounding;
use campfire_mode::{ChoiceData, ListEntry, ModeParam, Offers, OptionData, RelationData};
use campfire_navigation::ModeNavigationData;
use campfire_orders::AcquireData;
use campfire_package::ModePackages;
use campfire_package_format::ContentHash;
use campfire_production::{
    BuildData, BuildStart, ConstructData, FramePoint, ProductionData, RequiredUnit, RequiresData,
    ResearchData, ResearchHolder, TrainData,
};
use campfire_projectiles::{ProjectileData, ProjectileDeliveryData};
use campfire_sim::Capability;
use campfire_stats::{
    ModeStatsData, ModifiersData, StatOp, StatsData, TypePoolsData, UpgradeData, UpgradesData,
};
use campfire_store::internals::TestDir;
use campfire_units::TypeTagsData;
use campfire_values::{
    FilterData, Number, Ranked, Relation, RelationSet, Scalar, Share, Stat, TextId,
};

use super::*;
use crate::texture::samples::ktx2;
use crate::zero_hour::animation_import::{AnimationDropCause, ConvertedAnimation};
use crate::zero_hour::dropped::Dropped;
use crate::zero_hour::error::{NameClash, NameError, VersionDifference};
use crate::zero_hour::format::big::big_archive::samples::big;
use crate::zero_hour::format::csf::string_table::StringTable;
use crate::zero_hour::format::csf::string_table::samples::strings;
use crate::zero_hour::format::ini::ai_data_ini::samples::AI_DATA;
use crate::zero_hour::format::ini::armor_ini::samples::ARMORS;
use crate::zero_hour::format::ini::command_button_ini::samples::COMMAND_BUTTONS;
use crate::zero_hour::format::ini::command_set_ini::samples::COMMAND_SETS;
use crate::zero_hour::format::ini::game_data_ini::GameDataIni;
use crate::zero_hour::format::ini::game_data_ini::TerrainDraw;
use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::format::ini::object_ini::samples::{DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS};
use crate::zero_hour::format::ini::player_template_ini::samples::PLAYER_TEMPLATES;
use crate::zero_hour::format::ini::upgrade_ini::samples::UPGRADES;
use crate::zero_hour::format::ini::weapon_ini::samples::WEAPONS;
use crate::zero_hour::format::map::map_file::samples::map;
use crate::zero_hour::install::samples::{FixtureInstall, fixture, packed_map};
use crate::zero_hour::look_flags::FlagCondition;
use crate::zero_hour::model_bones::ModelBones;
use crate::zero_hour::model_import::{Converted, ModelImport};
use crate::zero_hour::model_writer::{DroppedStates, SkipReason, SkippedModel, StateDropCause};
use crate::zero_hour::object_flight::samples::shell_projectile;
use crate::zero_hour::production_import::ButtonDropCause;
use crate::zero_hour::rules_names::RulesNames;
use crate::zero_hour::upgrade_import::UpgradeDropCause;
use crate::zero_hour::weapon_drop_cause::WeaponDropCause;

/// The fixture install's version, as its archives hold their bytes now.
fn version_of(test_dir: &TestDir) -> &'static [GameVersion] {
    let archives = ["A.big", "b.big", "c.BIG", "ZH_Generals/base.big"]
        .map(|path| (path, ContentHash::of(&test_dir.read(format!("zh/{path}")))));
    GameVersion::fixture("fixture", archives)
}

#[test]
fn an_install_of_a_known_version_imports_its_maps_and_another_does_not() {
    let test_dir = TestDir::new();
    fixture(&test_dir);
    let known = version_of(&test_dir);
    let zero_hour = ZeroHour::open_among(&test_dir.path("zh"), known).unwrap();
    assert_eq!(zero_hour.version().name(), "fixture");

    // The fixture map, in its directory `Fixture Map`, is `fixture_map`: its files are the ones
    // its import gives, and the unit types it places the package's.
    let imported = zero_hour.write(&test_dir.path("one")).unwrap();
    let fingerprint = imported.fingerprint;
    // Its terrain's sand of 2 × 1 texels holds no tile, and its grass has no texture, so each
    // of the 3 × 2 cells the game draws is untextured.
    let named = MapName::parse("fixture_map").unwrap();
    assert_eq!(imported.maps, std::slice::from_ref(&named));
    let untextured = ImportLoss::UntexturedCells {
        map: named.clone(),
        cells: 6,
    };
    assert!(imported.losses.contains(&untextured));
    assert!(
        !imported
            .losses
            .iter()
            .any(|loss| matches!(loss, ImportLoss::Waypoint { .. }))
    );
    // Beside it, the rules package as the workspace holds it, which the imported mode, of
    // its manifest and mode data, depends on and loads with.
    assert_eq!(test_dir.names("one"), ["game", "zero-hour"]);
    assert_eq!(
        test_dir.names("one/game"),
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
        test_dir.read("one/zero-hour/package.index"),
        include_bytes!("../../../../../packages/zero-hour/package.index")
    );
    // The package's own reader takes its units file: the tank, the wall and the rocks the map
    // places, America's start objects, the barracks, the power plant, the ranger and the
    // pathfinder their command sets make, and the shell the tank's cannon fires. Its units drive, by stats, orders and
    // navigation, no faster than the tank's 30.5 m a second.
    let mode = ModePackages::at(&PackageDir::new(test_dir.path("one/game")), Some(&named)).unwrap();
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
            "americainfantrypathfinder",
            "americainfantryranger",
            "americapowerplant",
            "americatankcrusader",
            "americavehicledozer",
            "fortresswall",
            "rocks1",
            "tankshell"
        ]
    );
    // Its players pick America, the fixture's one faction, of the choice `faction`, and start
    // with its command center and dozer, the dozer half the center's sphere of 25 m, 12.5 m,
    // along +z where a map has no rally, at the markers tagged `start`, which the load checks a
    // walker can stand on, and with the money its host picks of the fixture's three choices,
    // 10,000 by default.
    // Its mode data lies over the rules package's, as the load lays them.
    let data = mode.data();
    let declared = |name: &str| DeclaredName::new(name).unwrap();
    let all_declared =
        |names: &[&str]| names.iter().map(|&name| declared(name)).collect::<Vec<_>>();
    let one = |value: i64| Ranked::One(Number::from(value));
    let navigation = data.section::<ModeNavigationData>().unwrap();
    assert_eq!(navigation.spawn_tags, all_declared(&["start", "rally"]));
    // Its base, as the dozer builds the power plant, which America's science lets it, and the
    // barracks, and the center and the barracks train, and two of each unit (09-zero-hour.md, A base start); its army the ranger alone, as the sample's
    // pathfinder holds no weapon.
    let names = |names: &[&str]| {
        ModeParam::List(
            names
                .iter()
                .map(|&name| ListEntry::Name(declared(name)))
                .collect(),
        )
    };
    assert_eq!(
        data.params,
        BTreeMap::from([
            (
                declared("army_units_america"),
                names(&["americainfantryranger"])
            ),
            (
                declared("base_buildings_america"),
                names(&["americapowerplant", "americabarracks"])
            ),
            (declared("base_count"), ModeParam::Value(Scalar::Int(2))),
            // The sample's dozer builds and does not gather, and no building takes a load.
            (declared("drop_offs_america"), names(&[])),
            (declared("gatherers_america"), names(&[])),
            // America starts with its science, which the mode's start grants, and its bot buys
            // its side's two skill sets, a list each.
            (declared("perks_america"), names(&["science_america"])),
            (
                declared("skill_sets_america"),
                names(&["skill_set_1_america", "skill_set_2_america"])
            ),
            (
                declared("skill_set_1_america"),
                names(&["science_paradrop1", "science_paradrop2"])
            ),
            (
                declared("skill_set_2_america"),
                names(&["science_paradrop2"])
            ),
            (
                declared("base_units_america"),
                names(&[
                    "americavehicledozer",
                    "americainfantryranger",
                    "americainfantrypathfinder"
                ])
            ),
            (
                declared("start_building_america"),
                ModeParam::Name(declared("americacommandcenter"))
            ),
            (
                declared("start_offset_america"),
                ModeParam::Value(Scalar::Decimal(Num::int(25) / Num::int(2)))
            ),
            (
                declared("start_units_america"),
                ModeParam::List(vec![ListEntry::Name(declared("americavehicledozer"))])
            ),
        ])
    );
    let choice = |name: &str, values: &[&str]| ChoiceData {
        name: TextId::new(name).unwrap(),
        offers: Offers::Values,
        unique: false,
        count: NonZeroU8::MIN,
        slot: None,
        values: all_declared(values),
    };
    assert_eq!(
        data.choices,
        BTreeMap::from([
            (declared("faction"), choice("choice-faction", &["america"])),
            (
                declared("start"),
                choice("choice-start", &["standard", "base"])
            ),
        ])
    );
    let money = [5000, 10000, 20000].map(Scalar::Int).into();
    assert_eq!(
        data.options,
        BTreeMap::from([(
            declared("start_money"),
            OptionData::new(money, Scalar::Int(10000)).unwrap()
        )])
    );
    // Each of the eight players regards the neutral team as neutral, as the original's players
    // regard its neutral player.
    let neutral: Vec<RelationData> = (1..=8)
        .map(|player| RelationData {
            teams: [declared(&format!("player_{player}")), declared("neutral")],
            relation: Relation::Neutral,
            vision: false,
        })
        .collect();
    assert_eq!(data.relations, neutral);
    let slots = data.section::<SlotKinds>().unwrap().kinds();
    let slots: Vec<&str> = slots.iter().map(|kind| kind.name.as_str()).collect();
    assert_eq!(slots, ["weapon", "command", "use"]);
    // The command sets' trains and builds: the center's dozer, the dozer's barracks, which
    // starts as it is ordered and its builder builds within 10 m, and the barracks' ranger,
    // which needs a barracks; each its object's cost, and the milliseconds of its frames, 5 s
    // 150 and 10 s 300, ⌊frames × 100 / 3⌋, which round back up to them.
    let made = |id: &str, cost: i64, windup_ms: i64, action: ActionData| {
        let name = format!("action-{}", id.replace('_', "-"));
        let action = ActionData {
            keys: ActionKeys {
                name: Some(TextId::new(&name).unwrap()),
                cost: BTreeMap::from([(declared("money"), one(cost))]),
                windup_ms: Some(one(windup_ms)),
                ..action.keys
            },
            ..action
        };
        (declared(id), action)
    };
    let meters = |range: i64| {
        let range = RangeField::Range(ActionRange::meters(Num::int(range)));
        Some(Ranked::One(range))
    };
    let train = |unit_type: &str, requires: Option<RequiresData>| {
        let train = TrainData {
            unit_type: declared(unit_type),
            requires,
            at_place: false,
        };
        ActionData::of(Targeting::None, train)
    };
    let barracks = BuildData {
        unit_type: declared("americabarracks"),
        requires: None,
        construct: ConstructData::Builder,
        start: BuildStart::Order,
        // The barracks' site starts at 1 of its 1,200 life, 833,333.3… billionths rounded up.
        start_life: Share::billionths(833_334),
        cancel_refund: None,
        placement: None,
    };
    let mut barracks = ActionData::of(Targeting::Point, barracks);
    barracks.keys.range = meters(10);
    // The power plant, of no cost and of no body, needs America's science: its perk's modifier
    // (generals.md, A perk's modifier is a player modifier).
    let power_plant = BuildData {
        unit_type: declared("americapowerplant"),
        requires: Some(RequiresData {
            units: Vec::new(),
            modifiers: vec![declared("science_america")],
        }),
        construct: ConstructData::Builder,
        start: BuildStart::Order,
        start_life: None,
        cancel_refund: None,
        placement: None,
    };
    let mut power_plant = ActionData::of(Targeting::Point, power_plant);
    power_plant.keys.range = meters(10);
    power_plant.keys.windup_ms = Some(one(1000));
    power_plant.keys.name = Some(TextId::new("action-build-americapowerplant").unwrap());
    let ranger = train(
        "americainfantryranger",
        Some(RequiresData {
            units: vec![RequiredUnit::from(declared("americabarracks"))],
            modifiers: Vec::new(),
        }),
    );
    // The pathfinder's line of two objects, either of which passes it.
    let either = ["americabarracks", "americacommandcenter"].map(declared);
    let pathfinder = train(
        "americainfantrypathfinder",
        Some(RequiresData {
            units: vec![RequiredUnit::any_of(either).unwrap()],
            modifiers: Vec::new(),
        }),
    );
    // The weapons of the starting sets that hit one target at once: the rifle, the ranger's and
    // the crusader's, once, its 5 damage of small arms within 100 m, a shot each 100 ms, 3
    // frames, `ceil(100 × 0.03)`, in a clip of 3 that reloads in 700 ms, 21 frames.
    let damage = |amount: i64| {
        let amount = Number::Value(Scalar::Decimal(Num::int(amount)));
        WeaponValue::Own(Ranked::One(amount))
    };
    let enemies = Targeting::Unit(FilterData::parse("enemies:!air_target").unwrap());
    // Each aims within the least of 0.035 radians, 0.035 × 180 ÷ π degrees, but the cannon's 30°.
    let radians: Num = "0.035".parse().unwrap();
    let least = radians.checked_mul_div(Num::int(180), Num::PI, Rounding::NearestEven);
    // Each takes the veterancy's and the garrison's damage, and the garrison's range.
    let names = RulesNames::default();
    let fixed_damage = BonusData::of([names.veterancy_damage, names.garrison_damage]).unwrap();
    let garrison_range = BonusData::from(names.garrison_range);
    let rifle = AttackData {
        rate: WeaponValue::Own(one(100)),
        rate_bonus: Some(BonusData::from(RulesNames::default().veterancy_rate)),
        damage: damage(5),
        damage_bonus: Some(fixed_damage.clone()),
        range_bonus: Some(garrison_range.clone()),
        damage_kind: declared("small_arms"),
        prefers: None,
        clip: Some(ClipData {
            shots: one(3),
            reload_ms: one(700),
            refill: ClipRefill::Time,
        }),
        splash: None,
        scatter: None,
        pitch_deg: None,
        speed: None,
        streak: None,
        pre_attack: None,
        aim_deg: least,
    };
    let mut rifle = ActionData::of(enemies.clone(), rifle);
    rifle.keys.range = meters(100);
    // The crusader's cannon fires its shell, of 60 armor-piercing damage within 5 m of where it
    // ends, to all but its own unit, which scatters 10 m at infantry, within 15° of level.
    let cannon = AttackData {
        rate: WeaponValue::Own(one(2000)),
        rate_bonus: Some(BonusData::from(RulesNames::default().veterancy_rate)),
        damage: damage(60),
        damage_bonus: Some(fixed_damage),
        range_bonus: Some(garrison_range.clone()),
        damage_kind: declared("armor_piercing"),
        prefers: None,
        clip: None,
        splash: SplashData::new(Num::int(5), RelationSet::All, true),
        scatter: ScatterData::new(
            Num::ZERO,
            BTreeMap::from([(declared("infantry"), Num::int(10))]),
        ),
        pitch_deg: PitchData::new(Num::int(-15), Num::int(15)),
        speed: None,
        streak: None,
        pre_attack: None,
        aim_deg: Some(Num::int(30)),
    };
    let mut cannon = ActionData::of(enemies.clone(), cannon);
    cannon.keys.range = meters(150);
    cannon.delivery = Some(Arc::new(ProjectileDeliveryData::one(declared("tankshell"))));
    // The bolt of the ranger's upgraded set alone, while it holds the weapon set's tag: 30
    // explosive damage within 100 m at 2,400 m/s, a shot each 1,000 ms, 30 frames; its upgrade's
    // 125 % names the stat of that percent.
    let bolt = AttackData {
        rate: WeaponValue::Own(one(1000)),
        rate_bonus: Some(BonusData::from(RulesNames::default().veterancy_rate)),
        damage: damage(30),
        damage_bonus: BonusData::of([
            RulesNames::default().veterancy_damage,
            RulesNames::default().garrison_damage,
            Stat::named("upgrade_damage_125").unwrap(),
        ]),
        range_bonus: Some(garrison_range),
        damage_kind: declared("explosion"),
        prefers: None,
        clip: None,
        splash: None,
        scatter: None,
        pitch_deg: None,
        speed: Speed::new(Num::int(2400)),
        streak: None,
        pre_attack: None,
        aim_deg: least,
    };
    let mut bolt = ActionData::of(enemies, bolt);
    bolt.keys.range = meters(100);
    bolt.keys.when = Some(RulesNames::default().weapon_upgraded);
    // The center's and the barracks' researches, each of its upgrade's cost and time, 30 s, 15.5
    // s and 10 s, 900, 465 and 300 frames, ⌊frames × 100 / 3⌋ ms; the barracks' walls an
    // object's upgrade, which its producer holds.
    let research = |upgrade: &str, holder| {
        let research = ResearchData {
            modifier: declared(upgrade),
            holder,
            requires: None,
        };
        ActionData::of(Targeting::None, research)
    };
    let actions = &mode.content().section::<ActionsData>().unwrap().0;
    assert_eq!(
        *actions,
        BTreeMap::from([
            made("build_americabarracks", 600, 10000, barracks),
            (declared("build_americapowerplant"), power_plant),
            made("train_americainfantrypathfinder", 300, 6000, pathfinder),
            made("train_americainfantryranger", 225, 5000, ranger),
            made(
                "train_americavehicledozer",
                1000,
                5000,
                train("americavehicledozer", None)
            ),
            made(
                "research_upgrade_americabarrackswalls",
                400,
                10000,
                research("upgrade_americabarrackswalls", ResearchHolder::Producer)
            ),
            made(
                "research_upgrade_americacompositearmor",
                1000,
                30000,
                research("upgrade_americacompositearmor", ResearchHolder::Player)
            ),
            made(
                "research_upgrade_americarangerflashbang",
                500,
                15500,
                research("upgrade_americarangerflashbang", ResearchHolder::Player)
            ),
            (declared("weapon_bolt_upgraded"), bolt),
            (declared("weapon_rangerrifle"), rifle),
            (declared("weapon_tankcannon"), cannon),
        ])
    );
    // The game's 38 damage types, in its order, as the mode's kinds; life the pool `life`;
    // every armor, of each kind it does not take whole: the soldier's crushing and sniping
    // twice, the building's every kind by `DEFAULT` at half and small arms at a quarter, but
    // `UNRESISTABLE` and `SUBDUAL_UNRESISTABLE`, which pass whole.
    let combat = data.section::<ModeCombatData>().unwrap();
    assert_eq!(combat.damage_kinds.len(), 38);
    assert_eq!(
        [&combat.damage_kinds[0], &combat.damage_kinds[37]],
        [&declared("explosion"), &declared("status")]
    );
    assert_eq!(combat.life, Some(declared("life")));
    let share = |share: Num| ArmorShare::new(share).unwrap();
    assert_eq!(
        combat.armors[&declared("humanarmor")],
        BTreeMap::from([
            (declared("crush"), share(Num::int(2))),
            (declared("sniper"), share(Num::int(2))),
        ])
    );
    let building = &combat.armors[&declared("structurearmor")];
    assert_eq!(building.len(), 36);
    assert_eq!(
        building[&declared("explosion")],
        share(Num::int(1) / Num::int(2))
    );
    assert_eq!(
        building[&declared("small_arms")],
        share(Num::int(1) / Num::int(4))
    );
    assert!(!building.contains_key(&declared("unresistable")));
    assert!(!building.contains_key(&declared("subdual_unresistable")));
    // Each producer holds its converted buttons in its set's order; a trainer its queue, 9
    // by default, and its exit's points, the game's `y` its `−z`.
    let units = &mode.content().units;
    let unit = |name: &str| &units[&declared(name)];
    let slotted = |name: &str, kind: &str| {
        let slots = unit(name).section::<TypeSlotsData>();
        let slotted = slots.and_then(|slots| slots.0.get(&declared(kind)));
        slotted.cloned().unwrap_or_default()
    };
    let command = |name: &str| slotted(name, "command");
    assert_eq!(
        command("americacommandcenter"),
        all_declared(&[
            "train_americavehicledozer",
            "research_upgrade_americacompositearmor"
        ])
    );
    assert_eq!(
        command("americavehicledozer"),
        all_declared(&["build_americapowerplant", "build_americabarracks"])
    );
    assert_eq!(
        command("americabarracks"),
        all_declared(&[
            "train_americainfantryranger",
            "train_americainfantrypathfinder",
            "build_americapowerplant",
            "research_upgrade_americarangerflashbang",
            "research_upgrade_americabarrackswalls"
        ])
    );
    assert_eq!(command("americainfantryranger"), Vec::<DeclaredName>::new());
    // Each type's weapons, of its starting set and its upgraded set, in slot order, those
    // dropped left out: the ranger's rifle, in both of its sets, and its upgraded set's bolt
    // after its starting set's pistol, which drops; its life, its armor, and the command
    // center's tag of victory.
    let weapons = |name: &str| slotted(name, "weapon");
    assert_eq!(
        weapons("americainfantryranger"),
        all_declared(&["weapon_rangerrifle", "weapon_bolt_upgraded"])
    );
    assert_eq!(
        weapons("americatankcrusader"),
        all_declared(&["weapon_tankcannon", "weapon_rangerrifle"])
    );
    assert_eq!(weapons("americavehicledozer"), Vec::<DeclaredName>::new());
    // The crusader scans each 250 ms, written as 266, for the rules data's targets: a hostile unit
    // that is no structure, a hostile base defense, and a hostile container that holds riders;
    // the dozer's AI says `Yes` too, but it has no weapon, so it scans for none.
    let filters = [
        "hostiles:!structure",
        "hostiles:base_defense",
        "hostiles:holding",
    ];
    let acquire = AcquireData {
        scan_ms: 266,
        targets: filters.map(|text| FilterData::parse(text).unwrap()).into(),
    };
    let scan = |name: &str| unit(name).section::<AcquireData>().cloned();
    assert_eq!(scan("americatankcrusader"), Some(acquire));
    assert_eq!(scan("americavehicledozer"), None);
    let life_stat = Stat::named("life").unwrap();
    let life = |name: &str| {
        let unit = unit(name);
        let pools = unit.section::<TypePoolsData>();
        let stats = unit.section::<StatsData>();
        let life = stats.and_then(|stats| stats.value(&life_stat));
        (
            pools.map(|pools| pools.0.clone()),
            life.map(|life| life.base),
            unit.section::<CombatData>(),
        )
    };
    // A type's death and its turret's tolerance and slot kind are Zero Hour's rules data, which
    // lies under each type's own.
    let combat = |armor: Option<&str>| CombatData {
        on_death: OnDeath::Despawn,
        armor: armor.map(declared),
        armors: Vec::new(),
    };
    let lives = Some(all_declared(&["life"]));
    // The ranger's slow death keeps it 1001 ms, ⌈1001 · 0.03⌉ = 31 of the game's frames, 1033 ms.
    let kept = CombatData {
        on_death: OnDeath::Keep {
            ms: NonZeroU32::new(1033).unwrap(),
        },
        ..combat(Some("humanarmor"))
    };
    assert_eq!(
        life("americainfantryranger"),
        (lives.clone(), Some(Num::int(180)), Some(&kept))
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
    let tags = |name: &str| {
        unit(name)
            .section::<TypeTagsData>()
            .map(|tags| tags.0.clone())
    };
    assert_eq!(
        tags("americacommandcenter"),
        Some(all_declared(&["counts_for_victory", "structure"]))
    );
    assert_eq!(tags("americabarracks"), Some(all_declared(&["structure"])));
    assert_eq!(
        tags("americainfantryranger"),
        Some(all_declared(&["infantry"]))
    );
    assert_eq!(
        tags("americatankcrusader"),
        Some(all_declared(&["vehicle"]))
    );
    // The crusader's turret turns 180° a second, starts an attack within the original's 0.035
    // radians of its aim, 2.00535… degrees, 33,644,228 · 2⁻²⁴, and carries its `PRIMARY` cannon
    // alone, at place 0, not its `SECONDARY` rifle; its shell, of no life or
    // body, flies at its cannon's 400 m/s, 2 m wide, on its curve of 50% and 90%, 10 m up,
    // detonating on what is no player's own structure and on walls, for 10 s.
    assert_eq!(
        unit("americatankcrusader").section::<TurretData>(),
        TurretData::new(
            Num::int(180),
            Num::from_bits(33_644_228),
            declared("weapon")
        )
        .and_then(|turret| turret.carrying(vec![0]))
        .as_ref()
    );
    assert_eq!(life("tankshell"), (None, None, None));
    assert_eq!(tags("tankshell"), None);
    assert_eq!(
        unit("tankshell").section::<ProjectileData>(),
        Some(&shell_projectile(400))
    );
    // The weapons dropped, each by its cause, in the order the types are read: the ranger's
    // pistol of a random delay, the flamer of its upgraded set, of a radius and no projectile,
    // and the crusader's railgun no file defines. The wall's highlander
    // body gives no life; the dozer starts at 200 of its 250, which the engine starts full.
    let dropped: Vec<(&str, WeaponDropCause)> = imported
        .losses
        .iter()
        .filter_map(|loss| match loss {
            ImportLoss::Weapon(dropped) => Some((dropped.name.as_str(), dropped.cause)),
            _ => None,
        })
        .collect();
    assert_eq!(
        dropped,
        [
            ("Pistol", WeaponDropCause::RandomDelay),
            ("Flamer", WeaponDropCause::Radius),
            ("Railgun", WeaponDropCause::NoWeapon),
        ]
    );
    let named_by = |kind: fn(&ImportLoss) -> Option<&str>| {
        imported.losses.iter().filter_map(kind).collect::<Vec<_>>()
    };
    let unliving = named_by(|loss| match loss {
        ImportLoss::Unliving(object) => Some(object.as_str()),
        _ => None,
    });
    let wounded = named_by(|loss| match loss {
        ImportLoss::Wounded(object) => Some(object.as_str()),
        _ => None,
    });
    assert_eq!(
        (unliving, wounded),
        (vec!["FortressWall"], vec!["AmericaVehicleDozer"])
    );
    let point = |x: i64, z: i64| FramePoint::new([Num::int(x), Num::int(z)]);
    let trainer = |queue: u8, exit, rally| ProductionData {
        queue: NonZeroU8::new(queue).unwrap(),
        exit,
        rally,
        gathers: false,
        free: Vec::new(),
        places: Vec::new(),
    };
    let production = |name: &str| unit(name).section::<ProductionData>();
    assert_eq!(
        production("americacommandcenter"),
        Some(&trainer(9, point(-18, -35), point(60, -35)))
    );
    assert_eq!(
        production("americabarracks"),
        Some(&trainer(5, point(0, 0), point(55, 0)))
    );
    assert_eq!(production("americavehicledozer"), None);
    // The names, of the string table's labels, their hotkeys' markers dropped and `&&` an
    // `&`, braces escaped as Fluent reads them; the power plant's, the ranger's and the
    // pathfinder's labels the table lacks, so their objects' names.
    assert_eq!(
        test_dir.read_text("one/game/locale/en.ftl"),
        "mode-name = Zero Hour\nchoice-faction = Faction\nchoice-start = Start\nteam-player-1 = Player 1\nteam-player-2 = Player 2\n\
         team-player-3 = Player 3\nteam-player-4 = Player 4\nteam-player-5 = Player 5\n\
         team-player-6 = Player 6\nteam-player-7 = Player 7\nteam-player-8 = Player 8\n\
         team-neutral = Neutral\n\
         action-build-americabarracks = Barracks & {\"{\"}Yard{\"}\"}\n\
         action-build-americapowerplant = AmericaPowerPlant\n\
         action-research-upgrade-americabarrackswalls = Upgrade_AmericaBarracksWalls\n\
         action-research-upgrade-americacompositearmor = Upgrade_AmericaCompositeArmor\n\
         action-research-upgrade-americarangerflashbang = Upgrade_AmericaRangerFlashBang\n\
         action-train-americainfantrypathfinder = AmericaInfantryPathfinder\n\
         action-train-americainfantryranger = AmericaInfantryRanger\n\
         action-train-americavehicledozer = Dozer\n"
    );
    // The buttons dropped, each by its cause, in the order the sets are read: the dozer's, then
    // the barracks', which the dozer's set makes, the research of no upgrade last.
    let dropped: Vec<(&str, ButtonDropCause)> = imported
        .losses
        .iter()
        .filter_map(|loss| match loss {
            ImportLoss::Button(dropped) => Some((dropped.name.as_str(), dropped.cause)),
            _ => None,
        })
        .collect();
    assert_eq!(
        dropped,
        [
            ("Command_ConstructAmericaFireBase", ButtonDropCause::NoBody),
            (
                "Command_ConstructAmericaInfantryRanger",
                ButtonDropCause::NoQueue
            ),
            ("Command_ConstructFortressWall", ButtonDropCause::NoWalker),
            ("Command_ConstructGhost", ButtonDropCause::NoObject),
            ("Command_UpgradeLost", ButtonDropCause::NoObject),
        ]
    );
    // Each type's upgrades: the crusader's armor and its 100 more life by the composite armor;
    // the ranger's weapon set by the flash bang, and its weapon bonus by both, unless its
    // barracks walls, as a conflict no file defines goes; the barracks' 200 more life by its
    // own walls.
    let upgrades = |name: &str| unit(name).section::<UpgradesData>().cloned();
    let entry = |triggers: &[&str], all, conflicts: &[&str], modifier| UpgradeData {
        triggers: all_declared(triggers),
        all,
        conflicts: all_declared(conflicts),
        when: None,
        modifier: declared(modifier),
    };
    let [armor, flash, walls] = [
        "upgrade_americacompositearmor",
        "upgrade_americarangerflashbang",
        "upgrade_americabarrackswalls",
    ];
    assert_eq!(
        upgrades("americatankcrusader"),
        Some(UpgradesData(vec![
            entry(&[armor], false, &[], "upgrade_americacompositearmor_armor"),
            entry(
                &[armor],
                false,
                &[],
                "upgrade_americacompositearmor_life_100"
            ),
        ]))
    );
    assert_eq!(
        upgrades("americainfantryranger"),
        Some(UpgradesData(vec![
            entry(
                &[flash],
                false,
                &[],
                "upgrade_americarangerflashbang_weapons"
            ),
            entry(
                &[flash, armor],
                true,
                &[walls],
                "upgrade_americarangerflashbang_bonus"
            ),
        ]))
    );
    assert_eq!(
        upgrades("americabarracks"),
        Some(UpgradesData(vec![entry(
            &[walls],
            false,
            &[],
            "upgrade_americabarrackswalls_life_200"
        )]))
    );
    assert_eq!(upgrades("americavehicledozer"), None);
    // The modifiers: each upgrade's, of nothing of its own, and each effect's: the tags of the
    // armor set's and the weapon set's flag, the bonus's 125 % less 1, and the life added.
    let modifiers = &mode.content().section::<ModifiersData>().unwrap().0;
    let modifier = |name: &str| &modifiers[&declared(name)];
    let added = |name: &str, stat: &str| {
        let change = &modifier(name).stats[&Stat::named(stat).unwrap()];
        (change.op, change.value.clone())
    };
    let value = |value: Num| Number::Value(Scalar::Decimal(value));
    for upgrade in [armor, flash, walls] {
        let held = modifier(upgrade);
        assert!(held.tags.is_empty() && held.stats.is_empty(), "{upgrade}");
    }
    assert_eq!(
        modifier("upgrade_americacompositearmor_armor").tags,
        all_declared(&["armor_upgrade"])
    );
    assert_eq!(
        modifier("upgrade_americarangerflashbang_weapons").tags,
        all_declared(&["weapon_upgrade"])
    );
    assert_eq!(
        added("upgrade_americarangerflashbang_bonus", "upgrade_damage_125"),
        (StatOp::Add, value(Num::int(1) / Num::int(4)))
    );
    assert_eq!(
        added("upgrade_americacompositearmor_life_100", "life"),
        (StatOp::Add, Number::from(100))
    );
    assert_eq!(
        added("upgrade_americabarrackswalls_life_200", "life"),
        (StatOp::Add, Number::from(200))
    );
    assert!(
        data.section::<ModeStatsData>()
            .unwrap()
            .0
            .contains_key(&Stat::named("upgrade_damage_125").unwrap())
    );
    // The modules dropped, in the order of the types' names, and the crusader for its set of a
    // crate's condition: the ranger's armor of an upgrade no file defines, the crusader's life of
    // a change that keeps its share, and its stealth, an effect of none of the four, and the
    // bonus of its shell, a projectile's type.
    let dropped: Vec<(&str, UpgradeDropCause)> = imported
        .losses
        .iter()
        .filter_map(|loss| match loss {
            ImportLoss::Upgrade(dropped) => Some((dropped.name.as_str(), dropped.cause)),
            _ => None,
        })
        .collect();
    assert_eq!(
        dropped,
        [
            (
                "AmericaInfantryRanger ArmorUpgrade",
                UpgradeDropCause::NoUpgrade
            ),
            ("AmericaTankCrusader", UpgradeDropCause::Set),
            (
                "AmericaTankCrusader MaxHealthUpgrade",
                UpgradeDropCause::HealthChange
            ),
            (
                "AmericaTankCrusader StealthUpgrade",
                UpgradeDropCause::Effect
            ),
            ("TankShell WeaponBonusUpgrade", UpgradeDropCause::Projectile),
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
        Capability::Vision,
    ];
    // Eight players, each a team of one slot, and the neutral team of none.
    assert_eq!(manifest.teams.len(), 9);
    assert!(
        manifest.teams[..8]
            .iter()
            .all(|team| team.player_slots == 1)
    );
    assert!(
        drives
            .iter()
            .all(|&capability| manifest.capabilities.contains(capability))
    );
    assert_eq!(test_dir.names("one/game/map"), ["fixture_map"]);
    assert_eq!(
        test_dir.names("one/game/map/fixture_map"),
        ["heights.bin", "map.toml"]
    );
    assert_eq!(
        test_dir.names("one/game/client/maps/fixture_map"),
        [
            "conditions.toml",
            "lights.toml",
            "objects.toml",
            "terrain.bin"
        ]
    );
    // The class whose type's texture the install holds names it, the sign's TGA of 2 × 1
    // texels; the other names none.
    let file = ZeroHourMap::read(&map(8)).unwrap();
    let mut textures = vec![None; file.terrain.fields().classes.len()];
    textures[0] = Some(PackageTexture {
        path: PackagePath::parse("client/textures/art/terrain/tsand.ktx2").unwrap(),
        size: UVec2::new(2, 1),
    });
    let mut rules = ZeroHourRules {
        objects: ObjectIni::of(&[DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS]),
        ..ZeroHourRules::default()
    };
    rules.locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    rules.templates.read(PLAYER_TEMPLATES.as_bytes()).unwrap();
    rules.sets.read(COMMAND_SETS.as_bytes()).unwrap();
    rules.buttons.read(COMMAND_BUTTONS.as_bytes()).unwrap();
    rules.upgrades.read(UPGRADES.as_bytes()).unwrap();
    rules.strings = StringTable::read(&strings()).unwrap();
    rules.weapons.read(WEAPONS.as_bytes()).unwrap();
    rules.armors.read(ARMORS.as_bytes()).unwrap();
    rules.ai.read(AI_DATA.as_bytes()).unwrap();
    let looks: ClientUnits =
        Toml::parse(&test_dir.read_text("one/game/client/units.toml")).unwrap();
    let mut names = ImportNames::default();
    let roads = MapRoads::of(&file.objects, &rules.roads);
    let sources = MapSources {
        rules: &rules,
        names: &mut names,
        object_looks: looks.objects(),
        lights: 3,
        game: TerrainDraw {
            adjust_cliff_textures: true,
            three_way_blends: false,
        },
        textures: &textures,
        roads: &roads,
        road_textures: &[],
    };
    let expected = MapImport::new(&file, sources).unwrap();
    assert_eq!(
        test_dir.read_text("one/game/map/fixture_map/map.toml"),
        expected.map
    );
    assert_eq!(
        test_dir.read("one/game/map/fixture_map/heights.bin"),
        expected.heights
    );
    assert_eq!(
        test_dir.read("one/game/client/maps/fixture_map/terrain.bin"),
        expected.terrain
    );
    assert_eq!(
        test_dir.read("one/game/client/maps/fixture_map/lights.toml"),
        expected.lights
    );
    assert_eq!(
        test_dir.read("one/game/client/maps/fixture_map/objects.toml"),
        expected.objects
    );
    assert_eq!(
        test_dir.read("one/game/client/maps/fixture_map/conditions.toml"),
        expected.conditions
    );
    // The units file holds the map's types and the start's, which the start's own reading
    // of the templates adds, those the command sets make, as the import adds them, their
    // weapons, and the shell the crusader's cannon fires.
    SkirmishStart::of(&rules, &mut names).unwrap();
    let mut names = names.into_production();
    let production = ProductionImport::of(&rules, &mut names, BTreeMap::new(), BTreeMap::new());
    let mut names = names.into_projectiles();
    let combat = CombatImport::of(&rules, &mut names).unwrap();
    let upgrades = UpgradeImport::of(&rules, names.types(), &production, &combat).unwrap();
    let veterancy = VeterancyImport::of(&rules.game_data, names.types(), &rules.names).unwrap();
    let contains = ContainImport::of(&rules, names.types(), &BTreeMap::new()).unwrap();
    let types = names.close();
    let additions = TypeAdditions {
        weapon: &combat.weapons,
        weapon_slots: &combat.weapon_slots,
        command: &production.commands,
        production: &production.sections,
        sells: &production.sells,
        airfields: &production.airfields,
        nodes: &production.supply.nodes,
        drop_offs: &production.supply.drop_offs,
        spawns: &production.supply.spawns,
        upgrades: &upgrades.types,
        barrels: &BTreeMap::new(),
        bones: &BTreeMap::new(),
        tracks: &veterancy.tracks,
        experience: &veterancy.experience,
        areas: &production.areas,
        enters: &contains.enters,
        rides: &contains.rides,
    };
    let units = types.units_file(additions, &rules.names);
    assert_eq!(test_dir.read_text("one/game/data/units.toml"), units.text);
    let camera = Toml::parse::<CameraFile>(&test_dir.read_text("one/game/client/camera.toml"));
    assert_eq!(camera.unwrap().field_of_view, 50.0);
    // A press drags past `Mouse.ini`'s 4 pixels and clicks within its 167 ms.
    let keys = Toml::parse::<KeysFile>(&test_dir.read_text("one/game/client/keys.toml")).unwrap();
    let press = Press {
        drag: 4.0,
        click: 0.167,
    };
    assert_eq!(keys.press, press);
    let hud = Toml::parse::<HudFile>(&test_dir.read_text("one/game/client/hud.toml")).unwrap();
    assert_eq!(
        (hud.bars, hud.hit_mark),
        (ShownBars::Selected, HitMark::Hidden)
    );
    // America's panel of perks: its rank 3 set, the paradrop's first level at 1 and its second at
    // 4, of no icon, as the fixture maps no image; by the choice of a faction.
    let perks =
        Toml::parse::<PerksFile>(&test_dir.read_text("one/game/client/perks.toml")).unwrap();
    let place = |at, perk: &str| PerkPlace {
        place: NonZeroU8::new(at).unwrap(),
        perk: DeclaredName::new(perk).unwrap(),
        icon: None,
    };
    assert_eq!(perks.choice, DeclaredName::new("faction").unwrap());
    assert_eq!(
        perks.pages(&DeclaredName::new("america").unwrap()),
        [PerkPage {
            places: vec![place(1, "science_paradrop1"), place(4, "science_paradrop2")],
        }]
    );
    assert_eq!(
        (
            perks.grid.columns.get(),
            perks.grid.rows.get(),
            perks.grid.order
        ),
        (5, 3, PlaceOrder::Columns)
    );
    let picking = test_dir.read_text("one/game/client/picking.toml");
    assert_eq!(
        picking,
        "tiers = [\"all:!structure\"]\nleast_radius_px = 10.0\n"
    );
    let colors = test_dir.read_text("one/game/client/team_colors.toml");
    let colors = Toml::parse::<TeamColors>(&colors).unwrap();
    // The eight slots take the fixture's two colors in turn; the neutral team, the ninth, the
    // civilians' white, as the fixture's templates give it none.
    let (gold, red) = ([255, 200, 0], [255, 0, 0]);
    let mut slots = [gold, red].repeat(4);
    slots.push([255; 3]);
    assert_eq!(colors.colors(), slots);
    // Each texture a model or the terrain draws with once, at its path in the archives, as its
    // conversion gives it: the model's rock and the terrain's sand, whose TGA is the sign's; the
    // sign's own TGA, which nothing draws, is not converted.
    let [rock, sign] = ktx2();
    assert_eq!(
        test_dir.names("one/game/client/textures/art/textures"),
        ["rock.ktx2"]
    );
    assert_eq!(
        test_dir.read("one/game/client/textures/art/textures/rock.ktx2"),
        rock
    );
    assert_eq!(
        test_dir.read("one/game/client/textures/art/terrain/tsand.ktx2"),
        sign
    );
    // Two imports write the same bytes, and the package reads as the fingerprint names it.
    assert_eq!(
        zero_hour.write(&test_dir.path("two")).unwrap().fingerprint,
        fingerprint
    );
    assert_eq!(
        test_dir.read("two/game/package.index"),
        test_dir.read("one/game/package.index")
    );
    assert_eq!(
        PackageDir::new(test_dir.path("one/game"))
            .files()
            .unwrap()
            .fingerprint(),
        fingerprint
    );
}

#[test]
fn an_install_of_two_maps_of_one_name_or_of_an_unknown_version_does_not_import() {
    let test_dir = TestDir::new();
    fixture(&test_dir);
    // A second directory whose name gives `fixture_map` is refused.
    test_dir.write(
        "zh/c.BIG",
        big(&[
            ("c.ini", b"c"),
            ("Maps\\Fixture_Map\\Fixture_Map.map", &packed_map()),
        ]),
    );
    let known = version_of(&test_dir);
    let clash = ZeroHour::open_among(&test_dir.path("zh"), known).unwrap();
    assert!(matches!(
        clash.write(&test_dir.path("three")),
        Err(ZeroHourError::MapNameClash { first, second })
            if first.as_str() == "maps\\fixture map\\fixture map.map"
                && second.as_str() == "maps\\fixture_map\\fixture_map.map"
    ));

    // An archive of other bytes is no version the importer knows; the install is closed first,
    // as Windows replaces no file a program holds open.
    drop(clash);
    test_dir.write("zh/c.BIG", b"BIGF");
    assert!(matches!(
        ZeroHour::open_among(&test_dir.path("zh"), known),
        Err(ZeroHourError::Archive { .. })
    ));
    test_dir.write("zh/c.BIG", big(&[("c.ini", b"C")]));
    assert!(matches!(
        ZeroHour::open_among(&test_dir.path("zh"), known),
        Err(ZeroHourError::UnknownVersion {
            archive,
            difference: VersionDifference::OtherBytes,
        }) if archive == "c.big"
    ));
}

#[test]
fn an_import_writes_each_model_once_and_each_object_s_models() {
    let test_dir = TestDir::new();
    fixture(&test_dir);
    let known = version_of(&test_dir);
    let zero_hour = ZeroHour::open_among(&test_dir.path("zh"), known).unwrap();
    let imported = zero_hour.write(&test_dir.path("one")).unwrap();
    // Each model the objects name once, as its conversion gives it; and
    // each object's models with the nodes its default look hides. The tank's turret is hidden
    // with the flash on the bone below it, which its muzzle flash hides as well; the crate's
    // box and the marker's `NULL` draw nothing; the tank's aggregate, the emitter, the absent
    // model, the tank's really damaged model and the tree's, which the fixture install lacks,
    // are skipped, in the order of their objects' names.
    let skipped: Vec<SkippedModel> = imported
        .losses
        .iter()
        .filter_map(|loss| match loss {
            ImportLoss::Model(skipped) => Some(skipped.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        skipped,
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
                name: "tank_d".to_owned(),
                reason: SkipReason::Missing,
            },
            SkippedModel {
                name: "ptoak01".to_owned(),
                reason: SkipReason::Missing,
            },
        ]
    );
    // The flash's texture is in no archive: its triangle is untextured, and the import says so.
    let textures: Vec<&str> = imported
        .losses
        .iter()
        .filter_map(|loss| match loss {
            ImportLoss::Texture(texture) => Some(texture.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(textures, ["gone.tga"]);
    assert_eq!(
        test_dir.names("one/game/client/models"),
        ["rock01.glb", "tank.glb"]
    );
    let install = Install::open(&test_dir.path("zh")).unwrap();
    let mut assets = ModelAssets::new(&install).unwrap();
    let Ok(Converted::Model(tank)) = ModelImport::convert("tank", &mut assets) else {
        panic!("the tank converts");
    };
    assert_eq!(
        test_dir.read("one/game/client/models/tank.glb"),
        tank.glb.encode()
    );
    // Each animation the tank's states name once, as its conversion gives it: its aim's, whose
    // hierarchy is its model's; its walk's, of a hierarchy no file holds, is dropped.
    assert_eq!(
        test_dir.names("one/game/client/animations"),
        ["tank.tank_aim.glb"]
    );
    let Ok(ConvertedAnimation::Glb { glb: aim, .. }) =
        ConvertedAnimation::of("tank.tank_aim", &mut assets)
    else {
        panic!("the tank's aim converts");
    };
    assert_eq!(
        test_dir.read("one/game/client/animations/tank.tank_aim.glb"),
        aim.encode()
    );
    let dropped: Vec<&Dropped<AnimationDropCause>> = imported
        .losses
        .iter()
        .filter_map(|loss| match loss {
            ImportLoss::Animation(dropped) => Some(dropped),
            _ => None,
        })
        .collect();
    assert_eq!(
        dropped,
        [&Dropped {
            name: "ghost.ghost_walk".to_owned(),
            cause: AnimationDropCause::NoHierarchy,
        }]
    );
    let states: Vec<&DroppedStates> = imported
        .losses
        .iter()
        .filter_map(|loss| match loss {
            ImportLoss::States(dropped) => Some(dropped),
            _ => None,
        })
        .collect();
    // The states dropped, each look's own: of a draw module that draws no model, the tank's and
    // its camo's debris, the camo's once as its own look and once as the pair's variation, and
    // the crate's, the marker's and the stump's only module; and of an object whose default
    // model the package lacks, the ghost's, the hydrant's and the tree's.
    let dropped = |object: &str, cause, states| DroppedStates {
        object: object.to_owned(),
        cause,
        states,
    };
    let (undrawn, missing) = (StateDropCause::Undrawn, StateDropCause::Missing);
    assert_eq!(
        states,
        [
            &dropped("Crate", undrawn, 1),
            &dropped("Ghost", missing, 1),
            &dropped("Hydrant", missing, 1),
            &dropped("Marker", undrawn, 1),
            &dropped("Stump", undrawn, 1),
            &dropped("Tank", undrawn, 1),
            &dropped("TankCamo", undrawn, 1),
            &dropped("TankCamo", undrawn, 1),
            &dropped("Tree", missing, 1),
        ]
    );
    // Each model's top, at its scale, on glTF's axes, `(x, y, z)` as `(x, z, −y)`. The tank's:
    // its hull flat at 0; its turret's pivot 3 up, its half turn about `y` keeping heights, and
    // its mesh's vertex no triangle uses 5 up in it, so 8; the flash on the turret's bone, 3 up,
    // its triangle flat; its skin's vertices 3, 3 and 4 up. So 8 whole, and 4 of its turret and
    // flash hidden. The rock's triangle is 1 m tall.
    let scaled = |name: &str, scale: f32, hide: &[&str]| ClientModel {
        rounds: Vec::new(),
        model: PackagePath::parse(&format!("client/models/{name}.glb")).unwrap(),
        scale: ModelScale::new(scale).unwrap(),
        hide: hide.iter().map(|&node| node.to_owned()).collect(),
        top: ModelTop::new(match (name, hide.contains(&"tank.turret")) {
            ("tank", false) => 8.0,
            ("tank", true) => 4.0,
            _ => scale,
        }),
        upgrades: Vec::new(),
    };
    let model = |name: &str, hide: &[&str]| scaled(name, 1.0, hide);
    let unit = ClientUnit::still;
    // No fixture object gives a `RadarPriority`, a garrison or a capture, so the minimap shows
    // none of them.
    let looks = |looks: Vec<(&str, ClientUnit)>| {
        looks
            .into_iter()
            .map(|(name, unit)| {
                let unit = ClientUnit {
                    radar: RadarShown::Never,
                    ..unit
                };
                (DeclaredName::new(name).unwrap(), unit)
            })
            .collect()
    };
    // The tank's states: its default, of its turret hidden, and with it the flash below; its
    // really damaged state, of a model the package lacks, so of none; and its night state, which
    // shows its turret, but not its first slot's muzzle flash, which each shot of its one barrel
    // of no recoil bone shows for a frame, 33 ms. Its debris module draws no model, so its look
    // has no draw of it. Its turret turns the node of its bone `Turret`, the model's pivot
    // `TURRET`, as the converted model names it; its red reskin's own draw module names no
    // turret.
    let hidden = || vec![model("tank", &["tank.turret", "tank.flash"])];
    // Each of its tank models shows its turret and hides its flash by its armor's sub-object
    // upgrade, which the condition of the upgrade's tag tests; of a state whose slot flashes at
    // it, the flash stays hidden, so out of the upgrade.
    let armor = DeclaredName::new("upgrade_americacompositearmor_subobjects").unwrap();
    let armored = |models: Vec<ClientModel>, hide: &[&str]| -> Vec<ClientModel> {
        let upgrade = NodeUpgrade {
            condition: armor.clone(),
            show: vec!["tank.turret".to_owned()],
            hide: hide.iter().map(|&node| node.to_owned()).collect(),
        };
        let armored = |model| ClientModel {
            upgrades: vec![upgrade.clone()],
            ..model
        };
        models.into_iter().map(armored).collect()
    };
    // The tank's own draw module shows its first slot's rounds, of its `WeaponHideShowBone`
    // `Turret`: one round of the turret, but the flash on its child pivot; its third slot names
    // no bone of rounds; its red reskin's module shows none.
    let rounded = |models: Vec<ClientModel>| -> Vec<ClientModel> {
        let rounds = ClipRounds {
            slot: 0,
            rounds: vec![vec!["tank.turret".to_owned()]],
        };
        let rounded = |model| ClientModel {
            rounds: vec![rounds.clone()],
            ..model
        };
        models.into_iter().map(rounded).collect()
    };
    let state = |sets: &[&[&str]], models, animation| LookState {
        animation,
        ..LookState::new(
            sets.iter()
                .map(|set| {
                    set.iter()
                        .map(|name| DeclaredName::new(name).unwrap())
                        .collect()
                })
                .collect(),
            models,
        )
    };
    // Its default plays its aim once, of the key `rest`, which its other states copy; its really
    // damaged state's idle walk, of a hierarchy no file holds, converts to none, so that state
    // plays none, and it rises by the tank's height, its default sphere's radius of 1 at its
    // scale of 1; its night state's key is `Lit-Up`, `lit_up` in the package. Between the two,
    // the default's copy plays its aim once backwards.
    let aim = |mode| {
        let clip = StateClip {
            animation: PackagePath::parse("client/animations/tank.tank_aim.glb").unwrap(),
            stride: None,
        };
        StateAnimation::new(vec![clip], mode, StatePlay::default())
    };
    let name = |name: &str| DeclaredName::new(name).unwrap();
    let keyed = |state: LookState, key: &str| LookState {
        key: Some(name(key)),
        ..state
    };
    let flashing = |state: LookState| LookState {
        fire_bones: vec![FireBone {
            slot: 0,
            barrels: vec![FireBarrel {
                bone: None,
                flash: Some(BarrelFlash {
                    node: "tank.flash".to_owned(),
                    ms: 33,
                }),
            }],
        }],
        ..state
    };
    let tank = || ClientUnit {
        turret: Some("turret".to_owned()),
        ..ClientUnit::new(vec![
            LookDraw::new(
                vec![
                    keyed(
                        flashing(state(
                            &[&[]],
                            rounded(armored(hidden(), &[])),
                            aim(AnimationMode::Once),
                        )),
                        "rest",
                    ),
                    keyed(
                        LookState {
                            rise: Some(Num::ONE),
                            ..state(&[&["reallydamaged"]], Vec::new(), None)
                        },
                        "rest",
                    ),
                    keyed(
                        flashing(state(
                            &[&["night"]],
                            rounded(armored(vec![model("tank", &["tank.flash"])], &[])),
                            aim(AnimationMode::Once),
                        )),
                        "lit_up",
                    ),
                ],
                Vec::new(),
                vec![
                    LookTransition::new(
                        name("rest"),
                        name("lit_up"),
                        LookShown {
                            models: rounded(armored(hidden(), &[])),
                            animation: aim(AnimationMode::OnceBackwards),
                            rise: None,
                        },
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        ])
    };
    // The rock and its reskin at the rock's scale, which the reskin cannot set; the tree's
    // look as a unit of both its models, and as the client's of its tree draw module's; the
    // fluff's as a unit of its hidden turret, with the flash below it, and as the client's of
    // none. An object of no draw module, of a model of `NONE`, of `NULL` or of a box draws
    // nothing, so its look is empty; one of a model the package lacks has none. The command
    // center's look keeps the angle its build's preview places it at.
    let rock = || unit(vec![scaled("rock01", 1.5, &[])]);
    let empty = || ClientUnit::new(Vec::new());
    let objects = ObjectIni::of(&[]);
    let flags = LookFlags::of(&GameDataIni::default(), &RulesNames::default(), &objects).unwrap();
    let test = |flag: &str| {
        let FlagCondition { name, test } = flags.condition(flag).unwrap();
        (name, test.expect("a tested flag"))
    };
    let armed = |slots: &[&str]| ClientUnit {
        weapons: slots
            .iter()
            .map(|weapon| vec![DeclaredName::new(weapon).unwrap()])
            .collect(),
        ..empty()
    };
    // The panels of the sets: each converted button at its place, the command center's rally at
    // its 13, its second research of one action and the barracks' wall left out (02-engine-core.md,
    // A look places its buttons).
    let declared = |name: &str| DeclaredName::new(name).unwrap();
    let panel = |places: &[(u8, &str)]| {
        let places = places.iter().map(|&(place, button)| PanelPlace {
            place: NonZeroU8::new(place).unwrap(),
            button: match button {
                "rally" => PanelButton::Command(PanelCommand::Rally),
                action => PanelButton::Action(declared(action)),
            },
        });
        PanelPlaces::try_from(places.collect::<Vec<_>>()).unwrap()
    };
    let expected = ClientUnits::new(
        looks(vec![
            (
                "americabarracks",
                ClientUnit {
                    panel: panel(&[
                        (1, "train_americainfantryranger"),
                        (2, "train_americainfantrypathfinder"),
                        (6, "build_americapowerplant"),
                        (7, "research_upgrade_americarangerflashbang"),
                        (8, "research_upgrade_americabarrackswalls"),
                    ]),
                    ..empty()
                },
            ),
            (
                "americacommandcenter",
                ClientUnit {
                    placement_angle: Some(Num::int(-135)),
                    turret: None,
                    panel: panel(&[
                        (1, "train_americavehicledozer"),
                        (2, "research_upgrade_americacompositearmor"),
                        (13, "rally"),
                    ]),
                    ..empty()
                },
            ),
            ("americafirebase", empty()),
            ("americainfantrypathfinder", empty()),
            // The Ranger's rifle, of both its sets, in slot A, and its upgrade set's bolt in slot
            // B, the groups its look's attack conditions test; the Crusader's cannon in A and its
            // rifle in B.
            (
                "americainfantryranger",
                armed(&["weapon_rangerrifle", "weapon_bolt_upgraded"]),
            ),
            ("americapowerplant", empty()),
            (
                "americatankcrusader",
                armed(&["weapon_tankcannon", "weapon_rangerrifle"]),
            ),
            (
                "americavehicledozer",
                ClientUnit {
                    panel: panel(&[(1, "build_americapowerplant"), (3, "build_americabarracks")]),
                    ..empty()
                },
            ),
            ("bush", unit(hidden())),
            ("crate", empty()),
            ("fortresswall", empty()),
            ("marker", empty()),
            (
                "oak",
                ClientUnit::new(vec![
                    LookDraw::still(vec![model("tank", &[])]),
                    LookDraw::still(vec![model("rock01", &[])]),
                ]),
            ),
            ("rock", rock()),
            ("rockred", rock()),
            ("rocks1", empty()),
            ("stump", empty()),
            ("tank", tank()),
            ("tankcamo", tank()),
            // The pair draws no model of its own, and each of its variations the look of its
            // object, its turret's node among it
            // (02-engine-core.md, A look varies by its unit's variation).
            (
                "tankpair",
                ClientUnit {
                    variations: vec![
                        LookVariation {
                            draws: tank().draws,
                            turret: tank().turret,
                            turret_pitch: tank().turret_pitch,
                        },
                        LookVariation {
                            draws: unit(armored(vec![model("tank", &[])], &["tank.flash"])).draws,
                            turret: None,
                            turret_pitch: None,
                        },
                    ],
                    ..empty()
                },
            ),
            (
                "tankred",
                unit(armored(vec![model("tank", &[])], &["tank.flash"])),
            ),
            ("tankshell", empty()),
        ]),
        ["night", "reallydamaged"]
            .map(test)
            .into_iter()
            .chain([(armor.clone(), ConditionTest::Tag { tag: armor.clone() })])
            .collect(),
        [
            ("bush", ObjectLook::new(vec![model("tank", &[])])),
            ("oak", ObjectLook::new(vec![model("rock01", &[])])),
            ("rock", ObjectLook::new(vec![scaled("rock01", 1.5, &[])])),
            ("rockred", ObjectLook::new(vec![scaled("rock01", 1.5, &[])])),
        ]
        .into_iter()
        .map(|(name, look)| (DeclaredName::new(name).unwrap(), look))
        .collect(),
    )
    .unwrap()
    // The buttons' keys, the letters their labels mark, the dozer's `d` and the barracks' `r`; the
    // fixture maps no image, so none has an icon.
    .with_actions(BTreeMap::from([
        (
            declared("build_americabarracks"),
            ActionLook {
                key: Some(Key::KeyR),
                ..ActionLook::default()
            },
        ),
        (
            declared("train_americavehicledozer"),
            ActionLook {
                key: Some(Key::KeyD),
                ..ActionLook::default()
            },
        ),
    ]));
    assert_eq!(
        Toml::parse::<ClientUnits>(&test_dir.read_text("one/game/client/units.toml")).unwrap(),
        expected
    );

    // Two objects whose names give one unit type, each with a model, are refused. The install is
    // closed first, as Windows replaces no file a program holds open.
    drop(assets);
    drop(install);
    drop(zero_hour);
    let clash = "Object Rock!\nDraw = W3DPropDraw M\nModelName = Rock01\nEnd\nEnd\nObject Rock?\nDraw = W3DPropDraw M\nModelName = Rock01\nEnd\nEnd\n";
    test_dir.write(
        "zh/c.BIG",
        big(&[
            ("c.ini", b"c"),
            ("Data\\INI\\Object\\Clash.ini", clash.as_bytes()),
        ]),
    );
    let known = version_of(&test_dir);
    let clash = ZeroHour::open_among(&test_dir.path("zh"), known).unwrap();
    assert!(matches!(
        clash.write(&test_dir.path("two")),
        Err(ZeroHourError::Name(NameError::Clash(NameClash { first, second })))
            if first == "Rock!" && second == "Rock?"
    ));
}

#[test]
fn each_drawn_texture_goes_once_and_two_that_share_a_file_clash() {
    // A texture drawn twice goes once; a DDS and a TGA of one name in one directory both go to
    // `x.ktx2`, which the import refuses, naming both.
    let path = ArchivePath::of;
    let files =
        texture_files([path("art\\a.dds"), path("art\\b.tga"), path("art\\a.dds")]).unwrap();
    let at = |text: &str| PackagePath::parse(text).unwrap();
    assert_eq!(
        files.into_iter().collect::<Vec<_>>(),
        [
            (at("client/textures/art/a.ktx2"), path("art\\a.dds")),
            (at("client/textures/art/b.ktx2"), path("art\\b.tga")),
        ]
    );
    let clash = texture_files([path("art\\x.dds"), path("art\\x.tga")]).unwrap_err();
    assert!(matches!(
        clash,
        ZeroHourError::TextureClash { first, second }
            if first == path("art\\x.dds") && second == path("art\\x.tga")
    ));
}

#[test]
fn a_types_bones_stand_as_its_first_variations_first_animation_places_them() {
    // The tank's turret bone, `(1, 2, 3)` turned a half turn about `z`, which its aim's first
    // frame leaves as it is: on glTF's axes `(x, z, −y)`, `(1, 3, −2)`. It names no `Barrel`
    // pivot, its pitch bone, and no launch bone, so no barrels. The pair of variations takes its
    // first's, the reskin's, which are the tank's; the red reskin's draw names no turret.
    let FixtureInstall { test_dir, install } = FixtureInstall::new();
    let rules = ZeroHourRules::read(&install).unwrap();
    let mut writer = PackageWriter::create(&test_dir.path("bones")).unwrap();
    let mut models = ModelWriter::new(ModelAssets::new(&install).unwrap(), &mut writer);
    let flags = LookFlags::of(&rules.game_data, &rules.names, &rules.objects).unwrap();
    let looks = models
        .units(&rules.objects, &mut ImportNames::default(), &flags)
        .unwrap();
    let tank = ModelBones {
        pivot: UnitOffset::new([1, 3, -2].map(Num::int)),
        ..ModelBones::default()
    };
    let bones: Vec<(&str, &ModelBones)> = looks
        .bones
        .iter()
        .map(|(name, bones)| (name.declared().as_str(), bones))
        .collect();
    assert_eq!(
        bones,
        [("tank", &tank), ("tankcamo", &tank), ("tankpair", &tank)]
    );
}
