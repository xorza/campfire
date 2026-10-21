use std::num::NonZeroU32;

use campfire_production::{DropOffData, EmptyNode, GatherData, NodeData, SpawnsData};
use campfire_values::{Distance, FilterData};

use super::*;
use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::supply_import::SupplyDropCause;

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

#[test]
#[expect(
    clippy::lossy_float_literal,
    reason = "71,582,784 and 71,582,792 are multiples of 8, which an f32 of 2²⁶ to 2²⁷ holds exactly"
)]
fn a_build_time_is_its_frames_in_milliseconds_within_the_games_int() {
    // 5 s is 150 frames, ⌊150 × 100 / 3⌋ = 5,000 ms; 0.1 s is 3 frames, 100 ms. 71,582,784 s
    // is 2,147,483,520 frames, the most an `f32` holds below 2³¹, ⌊2,147,483,520 × 100 / 3⌋
    // = 71,582,784,000 ms; 71,582,792 s, the next time an `f32` holds, is 2³¹ frames, past
    // the game's `Int`, and gives none, as what is not a time at all does.
    let windup = ProductionImport::windup_ms;
    assert_eq!(windup(5.0), Some(5000));
    assert_eq!(windup(0.1), Some(100));
    assert_eq!(windup(0.0), Some(0));
    assert_eq!(windup(71_582_784.0), Some(71_582_784_000));
    for refused in [71_582_792.0, f32::MAX, -1.0, f32::NAN, f32::INFINITY] {
        assert_eq!(windup(refused), None, "{refused}");
    }
}

/// A China supply center that trains and frees its truck, a dock that stays, a pile that goes,
/// a dock of no box, a helipad that frees a helicopter, a tunnel that trains nothing and frees
/// a guard, and a Helix whose AI module carries no box.
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
    Object Guard\n Locomotor = SET_NORMAL TankTreads\nEnd\n\
    Object Site\n Behavior = ProductionUpdate M1\n End\n\
     Behavior = SpawnBehavior M2\n  SpawnNumber = 3\n  SpawnReplaceDelay = 30000\n\
       SpawnTemplateName = Guard\n End\n\
    End\n\
    Object Shelter\n Behavior = SpawnBehavior M1\n  SpawnNumber = 2\n\
       SpawnTemplateName = Guard\n End\n\
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
    Object Helix\n Locomotor = SET_NORMAL JetWings\n Behavior = ChinookAIUpdate M1\n End\nEnd\n";

const SETS: &str = "CommandSet CenterSet\n 1 = Command_Truck\n 2 = Command_Jeep\n\
                      3 = Command_Kite\nEnd\n\
                    CommandSet HelipadSet\n 1 = Command_Heli\nEnd\n";

const BUTTONS: &str = "CommandButton Command_Truck\n Command = UNIT_BUILD\n Object = Truck\nEnd\n\
                       CommandButton Command_Jeep\n Command = UNIT_BUILD\n Object = Jeep\nEnd\n\
                       CommandButton Command_Kite\n Command = UNIT_BUILD\n Object = Kite\nEnd\n\
                       CommandButton Command_Heli\n Command = UNIT_BUILD\n Object = Heli\nEnd\n";

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
        "Helipad",
        "Tunnel",
        "Helix",
        "Site",
        "Burrow",
        "Shelter",
        "Crowd",
    ] {
        let object = rules.objects.object(placed.as_bytes()).unwrap();
        names.unit_type(object, &rules.locomotors).unwrap();
    }
    let import = ProductionImport::of(&rules, &mut names.into_production());
    let supply = &import.supply;
    let name = |name: &str| ImportName::of(name.as_bytes());
    let declared = |name: &str| DeclaredName::new(name).unwrap();

    // The dock holds its 400 boxes, 30,000, and stays; the pile its 150, 11,250, and goes.
    let node = |amount, empty| NodeData {
        resource: declared(MONEY),
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
        resources: vec![declared(MONEY)],
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
        ..ActionKeys::new(
            ActionKind::Gather,
            Targeting::Unit(FilterData::parse("all").unwrap()),
        )
    };
    let gather = GatherData {
        resource: declared(MONEY),
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
    // The jeep, of no locomotor, trains as one of its two variations, each a type of its own; the
    // kite, one of whose variations flies, drops (12-base-defenses.md, `BuildVariations` converts
    // to a train's variants).
    let MadeSection::Train(jeep) = &import.actions[&declared("train_jeep")].made else {
        panic!("the jeep's train");
    };
    assert_eq!(
        (&jeep.unit_type, &jeep.variants),
        (
            &declared("jeep"),
            &vec![declared("jeepone"), declared("jeeptwo")]
        )
    );
    let kite = DroppedButton {
        button: "Command_Kite".to_owned(),
        cause: ButtonDropCause::NoWalker,
    };
    assert!(import.dropped.contains(&kite), "{:?}", import.dropped);
    // The site of a production update keeps 3 guards, each back 30,000 ms, 900 frames, after
    // it dies, and trains nothing (12-base-defenses.md, A kept `SpawnBehavior` converts to
    // spawns).
    let spawns = SpawnsData {
        unit_type: declared("guard"),
        count: NonZeroU8::new(3).unwrap(),
        replace_ms: 30_000,
    };
    let site = &import.sections[&name("Site")];
    assert_eq!((&site.spawns, &site.free), (&Some(spawns), &Vec::new()));
    // The burrow, which trains nothing, frees its two guards by the production of its update.
    let burrow = &import.sections[&name("Burrow")];
    assert_eq!(
        (&burrow.spawns, &burrow.free),
        (&None, &vec![declared("guard"), declared("guard")])
    );
    // The dock of no box, the helicopter, which flies, the guard of a tunnel and of a shelter,
    // of no production, once for its cause, and the guard a crowd keeps none of are dropped, in
    // the order the types are visited; the Helix, of no box, is no gatherer, so nothing of it is
    // dropped.
    let dropped: Vec<(&str, SupplyDropCause)> = supply
        .dropped
        .iter()
        .map(|dropped| (dropped.object.as_str(), dropped.cause))
        .collect();
    assert_eq!(
        dropped,
        [
            ("Guard", SupplyDropCause::BadNumber),
            ("EmptyDock", SupplyDropCause::NoBoxes),
            ("Heli", SupplyDropCause::NoWalker),
            ("Guard", SupplyDropCause::NoExit),
        ]
    );
}
