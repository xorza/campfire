use std::num::NonZeroU32;

use campfire_actions::{ChannelData, ChannelPaysData};
use campfire_production::{DropOffData, EmptyNode, GatherData, NodeData, SpawnsData};
use campfire_values::{Distance, FilterData, Scalar};

use super::*;
use crate::zero_hour::dropped::Dropped;
use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;
use crate::zero_hour::format::ini::object_ini::build_variations::VariationDropCause;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::supply_import::{ImportedGather, SupplyDropCause};

#[test]
fn a_factions_base_is_what_its_start_builds_and_trains_once_each_but_its_faction_building() {
    // The dozer builds the center, which the start already holds, the barracks twice, by two
    // buttons, and gathers, which makes nothing; the worker the barracks again and the factory.
    // The center trains the dozer, the barracks the ranger, the dozer again and the mortar, the
    // factory the tank, the truck, which gathers, the scout, the technical, the mob, which leads
    // rangers, and the rabble, whose rangers act on their own: the base is the barracks and the
    // factory, and the dozer, the ranger, the mortar, the tank, the truck, the scout, the
    // technical, the mob and the rabble, in that order (09-zero-hour.md, A base start); its army
    // the ranger, the tank, the technical and the mob, which neither build nor gather and a
    // weapon arms, the mob's by its rangers (09-zero-hour.md, A leader of armed spawns is
    // armed), but the scout, of no weapon, the mortar, whose weapon dropped, and the rabble,
    // whose rangers fight in no place of its; its gatherers the dozer and the truck; and its
    // drop-offs the center and the factory, which take a load.
    let name = |text: &str| DeclaredName::new(text).unwrap();
    let train = |unit: &str| ImportedAction {
        keys: ActionKeys::new(Targeting::None),
        made: MadeSection::Train(TrainData {
            unit_type: name(unit),
            requires: None,
        }),
    };
    let build = |unit: &str| ImportedAction {
        keys: ActionKeys::new(Targeting::Point),
        made: MadeSection::Build(BuildData {
            unit_type: name(unit),
            requires: None,
            construct: ConstructData::Builder,
            start: BuildStart::Order,
            start_life: None,
            cancel_refund: None,
            placement: None,
        }),
    };
    let mut import = ProductionImport::default();
    for (id, action) in [
        ("build_center", build("center")),
        ("build_barracks", build("barracks")),
        ("raise_barracks", build("barracks")),
        ("build_factory", build("factory")),
        ("train_dozer", train("dozer")),
        ("train_ranger", train("ranger")),
        ("train_tank", train("tank")),
        ("train_truck", train("truck")),
        ("train_mortar", train("mortar")),
        ("train_scout", train("scout")),
        ("train_technical", train("technical")),
        ("train_mob", train("mob")),
        ("train_rabble", train("rabble")),
    ] {
        import.actions.insert(name(id), action);
    }
    for (producer, ids) in [
        (
            "dozer",
            &["build_center", "build_barracks", "gather", "raise_barracks"][..],
        ),
        ("worker", &["build_barracks", "build_factory"]),
        ("center", &["train_dozer"]),
        ("barracks", &["train_ranger", "train_dozer", "train_mortar"]),
        (
            "factory",
            &[
                "train_tank",
                "train_truck",
                "train_scout",
                "train_technical",
                "train_mob",
                "train_rabble",
            ],
        ),
        ("truck", &["gather"]),
    ] {
        let ids = ids.iter().map(|&id| name(id)).collect();
        import.commands.insert(TypeName::named(producer), ids);
    }
    import
        .supply
        .gathers
        .insert(name("gather"), ImportedGather::of_box());
    let money = DropOffData {
        resources: vec![name("money")],
    };
    for drop_off in ["center", "factory"] {
        import
            .supply
            .drop_offs
            .insert(TypeName::named(drop_off), money.clone());
    }
    for (unit, led) in [("mob", true), ("rabble", false)] {
        let spawns = SpawnsData {
            unit_types: vec![name("ranger")],
            count: NonZeroU8::new(10).unwrap(),
            replace_ms: 30_000,
            led,
        };
        import.supply.spawns.insert(TypeName::named(unit), spawns);
    }
    let mut combat = CombatImport::default();
    for (unit, weapon) in [
        ("ranger", "weapon_rifle"),
        ("tank", "weapon_cannon"),
        ("technical", "weapon_gun"),
    ] {
        combat
            .weapons
            .insert(TypeName::named(unit), vec![name(weapon)]);
    }
    let start = |units: &[&str]| FactionStart {
        name: name("faction"),
        building: name("center"),
        units: units.iter().map(|&unit| name(unit)).collect(),
        offset: Num::ZERO,
    };
    let base = FactionBase::of(&import, &combat, &start(&["dozer", "worker"]));
    let names = |names: &[&str]| names.iter().map(|&text| name(text)).collect::<Vec<_>>();
    assert_eq!(
        base,
        FactionBase {
            buildings: names(&["barracks", "factory"]),
            units: names(&[
                "dozer",
                "ranger",
                "mortar",
                "tank",
                "truck",
                "scout",
                "technical",
                "mob",
                "rabble",
            ]),
            army: names(&["ranger", "tank", "technical", "mob"]),
            gatherers: names(&["dozer", "truck"]),
            drop_offs: names(&["center", "factory"]),
        }
    );
    // A start of no builder has no base buildings, and its faction building's trains alone.
    let alone = FactionBase::of(&import, &combat, &start(&[]));
    assert_eq!(
        (alone.buildings, alone.units),
        (Vec::new(), names(&["dozer"]))
    );
}

#[test]
fn a_sites_start_is_the_least_share_of_billionths_that_gives_one_life() {
    // 1 of 5,000 is 0.0002 exactly; of 1,200, 0.000833333… up to 833,334 billionths, which
    // gives 1.0000008; of 180, 5,555,555.5… up to 5,555,556; a life of 1 or less starts
    // whole.
    let share = |most: Num| ProductionImport::one_life(most);
    let billionths = |parts| Share::billionths(parts).unwrap();
    assert_eq!(share(Num::int(5000)), billionths(200_000));
    assert_eq!(share(Num::int(1200)), billionths(833_334));
    assert_eq!(share(Num::int(180)), billionths(5_555_556));
    assert_eq!(share(Num::ONE), Share::ALL);
    assert_eq!(share(Num::HALF), Share::ALL);
}

/// A China supply center that trains and frees its truck, a dock that stays, a pile that goes,
/// a dock of no box, a helipad that frees a helicopter, a tunnel that trains nothing and frees
/// a guard, a Helix whose AI module carries no box, and two hackers.
const SUPPLY: &str = "Object Center\n\
     CommandSet = CenterSet\n\
     Behavior = ProductionUpdate M1\n End\n\
     Behavior = SupplyCenterProductionExitUpdate M2\n  NaturalRallyPoint = X:52 Y:0 Z:0\n End\n\
     Behavior = SpawnBehavior M3\n  SpawnNumber = 1\n  SpawnTemplateName = Truck\n\
       OneShot = Yes\n End\n\
     Behavior = SupplyCenterDockUpdate M4\n End\n\
    End\n\
    Object Truck\n Locomotor = SET_NORMAL TankTreads\n\
     Behavior = SupplyTruckAIUpdate M1\n  MaxBoxes = 4\n  SupplyCenterActionDelay = 400\n\
       SupplyWarehouseActionDelay = 1000\n  SupplyWarehouseScanDistance = 700\n End\n\
    End\n\
    Object Dock\n Behavior = SupplyWarehouseDockUpdate M1\n  StartingBoxes = 400\n End\nEnd\n\
    Object Pile\n Behavior = SupplyWarehouseDockUpdate M1\n  StartingBoxes = 150\n\
       DeleteWhenEmpty = Yes\n End\nEnd\n\
    Object EmptyDock\n Behavior = SupplyWarehouseDockUpdate M1\n  StartingBoxes = 0\n End\n\
    End\n\
    Object DropZone\n Behavior = OCLUpdate M1\n  OCL = OCL_CargoPlane\n  MinDelay = 240000\n End\n\
    End\n\
    Object Helipad\n CommandSet = HelipadSet\n Behavior = ProductionUpdate M1\n End\n\
     Behavior = SpawnBehavior M2\n  SpawnNumber = 1\n  SpawnTemplateName = Heli\n\
       OneShot = Yes\n End\n\
    End\n\
    Object Heli\n Locomotor = SET_NORMAL JetWings\n\
     Behavior = ChinookAIUpdate M1\n  MaxBoxes = 8\n End\n\
    End\n\
    Object Tunnel\n Behavior = SpawnBehavior M1\n  SpawnNumber = 1\n\
       SpawnTemplateName = Guard\n  OneShot = Yes\n End\n\
    End\n\
    Object Guard\n Locomotor = SET_NORMAL TankTreads\n KindOf = INFANTRY\n\
     Body = ActiveBody M1\n  MaxHealth = 100\n  InitialHealth = 100\n End\nEnd\n\
    Object Haunt\n Behavior = ProductionUpdate M1\n End\n\
     Behavior = SpawnBehavior M2\n  SpawnNumber = 1\n  SpawnTemplateName = Wisp\n End\n\
    End\n\
    Object Wisp\n Locomotor = SET_NORMAL TankTreads\nEnd\n\
    Object Site\n Behavior = ProductionUpdate M1\n End\n\
     Behavior = SpawnBehavior M2\n  SpawnNumber = 3\n  SpawnReplaceDelay = 30000\n\
       SpawnTemplateName = Guard\n  SpawnTemplateName = Watch\n End\n\
    End\n\
    Object Watch\n Locomotor = SET_NORMAL TankTreads\n KindOf = INFANTRY\n\
     Body = ActiveBody M1\n  MaxHealth = 50\n  InitialHealth = 50\n End\nEnd\n\
    Object Shelter\n Behavior = SpawnBehavior M1\n  SpawnNumber = 2\n\
       SpawnTemplateName = Guard\n End\n\
    End\n\
    Object Crypt\n Behavior = ProductionUpdate M1\n End\n\
     Behavior = SpawnBehavior M2\n  SpawnNumber = 2\n  SpawnTemplateName = Ghoul\n End\n\
    End\n\
    Object Ghoul\n Locomotor = SET_NORMAL NoSuchTreads\nEnd\n\
    Object Mob\n Locomotor = SET_NORMAL TankTreads\n\
     Behavior = SpawnBehavior M1\n  SpawnNumber = 3\n  SpawnReplaceDelay = 30000\n\
       SpawnTemplateName = Guard\n  SpawnTemplateName = Watch\n  AggregateHealth = Yes\n End\n\
    End\n\
    Object Jeep\n BuildCost = 500\n BuildTime = 5.0\n\
     BuildVariations = JeepOne JeepTwo\nEnd\n\
    Object JeepOne\n Locomotor = SET_NORMAL TankTreads\nEnd\n\
    ObjectReskin JeepTwo JeepOne\nEnd\n\
    Object Kite\n BuildVariations = JeepOne KiteOne\nEnd\n\
    Object KiteOne\n Locomotor = SET_NORMAL JetWings\nEnd\n\
    Object Burrow\n Behavior = ProductionUpdate M1\n End\n\
     Behavior = SpawnBehavior M2\n  SpawnNumber = 2\n  SpawnTemplateName = Guard\n\
       OneShot = Yes\n End\n\
    End\n\
    Object Crowd\n Behavior = ProductionUpdate M1\n End\n\
     Behavior = SpawnBehavior M2\n  SpawnNumber = 0\n  SpawnTemplateName = Guard\n End\n\
    End\n\
    Object Helix\n Locomotor = SET_NORMAL JetWings\n Behavior = ChinookAIUpdate M1\n End\nEnd\n\
    Object Hacker\n Locomotor = SET_NORMAL TankTreads\n Behavior = HackInternetAIUpdate M1\n\
      UnpackTime = 7900\n  PackTime = 2100\n  PackUnpackVariationFactor = 0.5\n\
      CashUpdateDelay = 2000\n  CashUpdateDelayFast = 1800\n  RegularCashAmount = 5\n\
      VeteranCashAmount = 6\n  EliteCashAmount = 8\n  HeroicCashAmount = 10\n\
      XpPerCashUpdate = 1\n End\nEnd\n\
    Object Lamer\n Locomotor = SET_NORMAL TankTreads\n Behavior = HackInternetAIUpdate M1\n\
      CashUpdateDelay = 1000\n  CashUpdateDelayFast = 1000\n  PackUnpackVariationFactor = 0.5\n\
     End\nEnd\n";

const SETS: &str = "CommandSet CenterSet\n 1 = Command_Truck\n 2 = Command_Jeep\n\
                      3 = Command_Kite\n 4 = Command_Mob\nEnd\n\
                    CommandSet HelipadSet\n 1 = Command_Heli\nEnd\n";

const BUTTONS: &str = "CommandButton Command_Truck\n Command = UNIT_BUILD\n Object = Truck\nEnd\n\
                       CommandButton Command_Jeep\n Command = UNIT_BUILD\n Object = Jeep\nEnd\n\
                       CommandButton Command_Kite\n Command = UNIT_BUILD\n Object = Kite\nEnd\n\
                       CommandButton Command_Heli\n Command = UNIT_BUILD\n Object = Heli\nEnd\n\
                       CommandButton Command_Mob\n Command = UNIT_BUILD\n Object = Mob\nEnd\n";

#[test]
fn docks_centers_gatherers_and_free_units_take_the_originals_numbers_in_boxes_of_money() {
    // The shipped box of 75; the placed objects, as a map places them.
    let mut rules = ZeroHourRules::default();
    rules.objects.read(SUPPLY.as_bytes()).unwrap();
    rules.locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    rules.sets.read(SETS.as_bytes()).unwrap();
    rules.buttons.read(BUTTONS.as_bytes()).unwrap();
    rules
        .game_data
        .read(b"GameData\n ValuePerSupplyBox = 75\nEnd\n")
        .unwrap();
    let mut names = ImportNames::default();
    for placed in [
        "Center",
        "Dock",
        "Pile",
        "EmptyDock",
        "DropZone",
        "Helipad",
        "Tunnel",
        "Helix",
        "Site",
        "Burrow",
        "Shelter",
        "Crowd",
        "Haunt",
        "Crypt",
        "Hacker",
        "Lamer",
    ] {
        let object = rules.objects.object(placed.as_bytes()).unwrap();
        names.unit_type(object, &rules).unwrap();
    }
    let import = ProductionImport::of(&rules, &mut names.into_production());
    let supply = &import.supply;
    let name = |name: &str| TypeName::new(&ImportName::of(name.as_bytes())).unwrap();
    let declared = |name: &str| DeclaredName::new(name).unwrap();

    // The dock holds its 400 boxes, 30,000, and stays; the pile its 150, 11,250, and goes.
    let node = |amount, empty| NodeData {
        resource: declared("money"),
        amount: NonZeroU32::new(amount).unwrap(),
        empty,
    };
    let nodes: Vec<(&str, &NodeData)> = supply
        .nodes
        .iter()
        .map(|(name, node)| (name.as_str(), node))
        .collect();
    assert_eq!(
        nodes,
        [
            ("dock", &node(30_000, EmptyNode::Stays)),
            ("pile", &node(11_250, EmptyNode::Goes)),
        ]
    );
    // The center takes money, starts its gatherers and frees its truck, which it trains.
    let money = DropOffData {
        resources: vec![declared("money")],
    };
    assert_eq!(supply.drop_offs.get(&name("Center")), Some(&money));
    assert_eq!(supply.drop_offs.len(), 1);
    let center = &import.sections[&name("Center")];
    assert_eq!(
        (center.gathers, &center.free),
        (true, &vec![declared("truck")])
    );
    // The truck gathers 4 boxes of 75 a trip, a box each 1,000 ms, 30 frames, standing
    // 400 ms, 12 frames, at the center, within 10 m of a dock, and looking 700 m round its
    // dock for another; its gather is the last of its command slots.
    assert_eq!(import.commands[&name("Truck")], [declared("gather_truck")]);
    let keys = ActionKeys {
        windup_ms: Some(Ranked::One(Number::from(1000))),
        range: Some(Ranked::One(RangeField::Range(ActionRange::meters(
            Num::int(10),
        )))),
        ..ActionKeys::new(Targeting::Unit(FilterData::parse("all").unwrap()))
    };
    let gather = GatherData {
        resource: declared("money"),
        take: NonZeroU32::new(300).unwrap(),
        unit: NonZeroU32::new(75),
        bounce: Distance::new(Num::int(700)),
        deliver_ms: Some(400),
    };
    assert_eq!(
        supply.gathers[&declared("gather_truck")].table(),
        keys.table(&gather)
    );
    assert_eq!(supply.gathers.len(), 1);
    // The hacker hacks as its last command: a windup of its unpack, 7,900 ms, 237 frames, spread
    // by half each way, then a channel of no end that pays its regular 5 of money each 2,000 ms,
    // 60 frames, and makes an order wait its pack, 2,100 ms, 63 frames. The lamer, of no unpack,
    // no pack and no cash, pays 1, as the original falls through its levels to it, and spreads
    // nothing, as its windup is none.
    assert_eq!(import.commands[&name("Hacker")], [declared("hack_hacker")]);
    let hack = |windup_ms: Option<i64>, tick_ms: i64, wind_down_ms: Option<i64>, cash| {
        let ms = |ms: Option<i64>| ms.map(|ms| Ranked::One(Number::from(ms)));
        ActionKeys {
            windup_ms: ms(windup_ms),
            channel: Some(ChannelData {
                duration_ms: None,
                tick_ms: Ranked::One(Number::from(tick_ms)),
                damage_interrupts: false,
                windup_spread: Some(Ranked::One(Number::Value(Scalar::Decimal(Num::HALF)))),
                wind_down_ms: ms(wind_down_ms),
                pays: Some(ChannelPaysData {
                    resource: declared("money"),
                    track: None,
                    amount: vec![cash],
                }),
            }),
            ..ActionKeys::new(Targeting::None)
        }
    };
    let hacks: Vec<(&str, &ActionKeys)> = supply
        .hacks
        .iter()
        .map(|(name, keys)| (name.as_str(), keys))
        .collect();
    assert_eq!(
        hacks,
        [
            ("hack_hacker", &hack(Some(7900), 2000, Some(2100), 5)),
            ("hack_lamer", &hack(None, 1000, None, 1)),
        ]
    );
    // The jeep, of no locomotor, trains as its own type, which walks as its variations do; the
    // kite, one of whose variations flies, trains as its first variation alone
    // (04-capabilities/00-overview.md, A unit type may vary its units).
    for (train, unit) in [("train_jeep", "jeep"), ("train_kite", "kite")] {
        let MadeSection::Train(made) = &import.actions[&declared(train)].made else {
            panic!("{train}");
        };
        assert_eq!(made.unit_type, declared(unit));
    }
    let unalike = Dropped {
        name: "Kite".to_owned(),
        cause: VariationDropCause::Unalike,
    };
    let dropped: Vec<&Dropped<VariationDropCause>> =
        rules.objects.variations().dropped().into_iter().collect();
    assert_eq!(dropped, [&unalike]);
    // The site of a production update keeps 3 spawns of a guard and a watch in turn, each back
    // 30,000 ms, 900 frames, after it dies, and trains nothing, its production section of no
    // spawn (09-zero-hour.md, A kept `SpawnBehavior` converts to spawns;
    // 04-capabilities/production.md, Spawns take their types in turn).
    let spawns = SpawnsData {
        unit_types: vec![declared("guard"), declared("watch")],
        count: NonZeroU8::new(3).unwrap(),
        replace_ms: 30_000,
        led: false,
    };
    assert_eq!(import.supply.spawns.get(&name("Site")), Some(&spawns));
    assert_eq!(
        import.sections[&name("Site")].free,
        Vec::<DeclaredName>::new()
    );
    // The mob, which the center trains, walks and leads its 3 spawns of a guard and a watch in
    // turn, of no production section, as they bud from each other, and its train converts, as
    // it walks by them (09-zero-hour.md, The import converts a mob).
    let led = SpawnsData {
        led: true,
        ..spawns
    };
    assert_eq!(import.supply.spawns.get(&name("Mob")), Some(&led));
    assert!(!import.sections.contains_key(&name("Mob")));
    let MadeSection::Train(made) = &import.actions[&declared("train_mob")].made else {
        panic!("train_mob");
    };
    assert_eq!(made.unit_type, declared("mob"));
    // The burrow, which trains nothing, frees its two guards by the production of its update,
    // and keeps none.
    let burrow = &import.sections[&name("Burrow")];
    assert_eq!(burrow.free, [declared("guard"), declared("guard")]);
    assert_eq!(import.supply.spawns.get(&name("Burrow")), None);
    // The drop zone, whose objects come over time, the dock of no box, the helicopter, which
    // flies, the guard of a tunnel and of a shelter, of no production, once for its cause, the
    // guard a crowd keeps none of, the ghoul a crypt keeps, whose locomotor no file defines, so
    // it gives no type, and the wisp a haunt keeps, of no life, are dropped, in the order the
    // types are visited; the Helix, of no box, is no gatherer, so nothing of it is
    // dropped. Of the hacker go its cash past regular, its experience, its delay in a building
    // and its pack's spread; the lamer, whose cash is 1 at each level, whose delays are one and
    // who packs at once, loses nothing. The mob, which the walk visits last as the center trains
    // it, loses its spawns' own catch-up.
    let dropped: Vec<(&str, SupplyDropCause)> = (&supply.dropped)
        .into_iter()
        .map(|dropped| (dropped.name.as_str(), dropped.cause))
        .collect();
    assert_eq!(
        dropped,
        [
            ("Guard", SupplyDropCause::BadNumber),
            ("Ghoul", SupplyDropCause::NoType),
            ("DropZone", SupplyDropCause::ObjectsOverTime),
            ("EmptyDock", SupplyDropCause::NoBoxes),
            ("Hacker", SupplyDropCause::HackVeterancy),
            ("Hacker", SupplyDropCause::HackExperience),
            ("Hacker", SupplyDropCause::HackInBuilding),
            ("Hacker", SupplyDropCause::PackSpread),
            ("Wisp", SupplyDropCause::Unliving),
            ("Heli", SupplyDropCause::NoWalker),
            ("Guard", SupplyDropCause::NoExit),
            ("Mob", SupplyDropCause::LedSpawns),
        ]
    );
}

/// A converted button's type of build variations and the objects its prerequisites name join the
/// walk, so their own command sets convert too: the center trains the jeep and the rover, whose
/// one variation, the scout, fields each, and whose set trains the wisp, a command of both; the
/// wisp needs the lab, which no button makes and no map places, and whose set trains the ghost.
#[test]
fn a_variation_and_a_prerequisite_convert_their_own_command_sets() {
    const OBJECTS: &str = "\
        Object Center\n CommandSet = CenterSet\n Behavior = ProductionUpdate M1\n End\nEnd\n\
        Object Jeep\n BuildCost = 500\n BuildTime = 5.0\n BuildVariations = Scout\nEnd\n\
        Object Rover\n BuildCost = 600\n BuildTime = 5.0\n BuildVariations = Scout\nEnd\n\
        Object Scout\n Locomotor = SET_NORMAL TankTreads\n CommandSet = ScoutSet\n\
         Behavior = ProductionUpdate M1\n End\nEnd\n\
        Object Wisp\n Locomotor = SET_NORMAL TankTreads\n BuildCost = 100\n BuildTime = 1.0\n\
         Prerequisites\n  Object = Lab\n End\nEnd\n\
        Object Lab\n CommandSet = LabSet\n Behavior = ProductionUpdate M1\n End\nEnd\n\
        Object Ghost\n Locomotor = SET_NORMAL TankTreads\n BuildCost = 100\n BuildTime = 1.0\n\
        End\n";
    const SETS: &str = "CommandSet CenterSet\n 1 = Command_Jeep\n 2 = Command_Rover\nEnd\n\
                        CommandSet ScoutSet\n 1 = Command_Wisp\nEnd\n\
                        CommandSet LabSet\n 1 = Command_Ghost\nEnd\n";
    const BUTTONS: &str = "\
        CommandButton Command_Jeep\n Command = UNIT_BUILD\n Object = Jeep\nEnd\n\
        CommandButton Command_Rover\n Command = UNIT_BUILD\n Object = Rover\nEnd\n\
        CommandButton Command_Wisp\n Command = UNIT_BUILD\n Object = Wisp\nEnd\n\
        CommandButton Command_Ghost\n Command = UNIT_BUILD\n Object = Ghost\nEnd\n";
    let mut rules = ZeroHourRules::default();
    rules.objects.read(OBJECTS.as_bytes()).unwrap();
    rules.locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    rules.sets.read(SETS.as_bytes()).unwrap();
    rules.buttons.read(BUTTONS.as_bytes()).unwrap();
    let mut names = ImportNames::default();
    let center = rules.objects.object(b"Center").unwrap();
    names.unit_type(center, &rules).unwrap();
    let import = ProductionImport::of(&rules, &mut names.into_production());
    let declared = |name: &str| DeclaredName::new(name).unwrap();
    for train in ["train_jeep", "train_rover", "train_wisp", "train_ghost"] {
        assert!(
            import.actions.contains_key(&declared(train)),
            "{train}: {:?}, {:?}",
            import.actions.keys().collect::<Vec<_>>(),
            import.dropped
        );
    }
    let wisp = [declared("train_wisp")];
    for parent in ["jeep", "rover"] {
        assert_eq!(import.commands[&TypeName::named(parent)], wisp, "{parent}");
    }
}
