use campfire_math::Num;

use super::*;
use crate::zero_hour::format::ini::object_ini::fight_data::{
    IniShell, IniTurret, IniTurretPitch, WeaponSet,
};
use crate::zero_hour::format::ini::object_ini::ini_animation::{AnimationFlag, IniClip, IniMode};
use crate::zero_hour::format::ini::object_ini::ini_build::{Prerequisite, ProductionExit};
use crate::zero_hour::format::ini::object_ini::ini_supply::{IniSpawn, SupplyDock, SupplyGatherer};
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::{HealthChange, UpgradeEffect};
use crate::zero_hour::format::ini::object_ini::samples::{DEFAULT_OBJECTS, OBJECTS};

/// Objects of each kind a placed object can be, their reskins, and lists of each form.
const PLACED: &str = "Object Oak
  KindOf = OPTIMIZED_TREE SHRUBBERY
End
Object Bench
  KindOf = prop
  Scale = 1.5
End
ObjectReskin BenchRed Bench
  KindOf = NONE
  Scale = 3.0
End
Object Bush
  KindOf = CLEARED_BY_BUILD SHRUBBERY
End
Object Gravel
  KindOf = CLEARED_BY_BUILD
  FenceWidth = 0.0
End
Object Fence
  KindOf = CLEARED_BY_BUILD
  FenceWidth = 10.0
End
ObjectReskin FenceDown Fence
  FenceWidth = 0.0
End
ObjectReskin FenceRed Fence
End
Object Tank
  KindOf = VEHICLE
End
Object Cleared
  KindOf = PROP
  KindOf = VEHICLE
End
Object Added
  KindOf = VEHICLE
  KindOf = +OPTIMIZED_TREE
End
Object Removed
  KindOf = PROP OPTIMIZED_TREE
  KindOf = -OPTIMIZED_TREE -PROP
End
Object Emptied
  KindOf = PROP
  KindOf = NONE PROP
End
Object Bridge
  IsBridge = Yes
End
ObjectReskin BridgeRed Bridge
  IsBridge = No
End
Object Wall
  KindOf = WALK_ON_TOP_OF_WALL PROP
End
Object Crossing
  IsBridge = no
End
";

/// The draw modules of the object `name`, each of its default state alone.
fn draws_of(ini: &ObjectFiles, name: &str) -> Vec<DrawModule> {
    let default = |draw: &DrawModule| DrawModule {
        model: draw.model.clone(),
        look: draw.look.clone(),
        turret: draw.turret.clone(),
        turret_pitch: draw.turret_pitch.clone(),
        rounds_shown: draw.rounds_shown,
        recoil: draw.recoil,
        ..DrawModule::new(draw.kind)
    };
    ini.named(name)
        .unwrap()
        .draws()
        .iter()
        .map(default)
        .collect()
}

#[test]
fn objects_give_their_default_models_and_looks_with_reskins_copied_as_read() {
    let mut ini = ObjectFiles::default();
    ini.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
    ini.read(OBJECTS.as_bytes()).unwrap();
    let names: Vec<&str> = ini.objects().map(|object| object.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "AmericaBarracks",
            "AmericaCommandCenter",
            "AmericaFireBase",
            "AmericaInfantryPathfinder",
            "AmericaInfantryRanger",
            "AmericaPowerPlant",
            "AmericaTankCrusader",
            "AmericaVehicleDozer",
            "Bush",
            "FortressWall",
            "Oak",
            "Rock",
            "RockRed",
            "Rocks1",
            "Stump",
            "Tank",
            "TankCamo",
            "TankPair",
            "TankRed",
            "TankShell",
            "Tree"
        ]
    );
    // The tank's default state: its model; its hides and shows, the second of `Barrel`
    // changing the first in its place; its weapon bones, a bone `None` none, and the sub-object
    // its first slot's rounds show; the bones its turret turns and pitches, `Turret` and
    // `Barrel`; the slots whose rounds it shows, the first and the third; its recoil, of its speed
    // of a second and its damping, its most the default; and the debris draw module names no
    // model.
    let model = |model: &str, look| DrawModule {
        model: Some(model.to_owned()),
        look,
        ..DrawModule::new(DrawKind::Family)
    };
    let change = |name: &str, hide| HideShow {
        name: name.to_owned(),
        hide,
    };
    let tank_look = StateLook {
        hide_show: vec![
            change("Turret", true),
            change("Barrel", true),
            change("Hatch", true),
        ],
        weapons: [
            WeaponBones {
                muzzle_flash: Some("MuzzleFX".to_owned()),
                rounds: Some("Turret".to_owned()),
                ..WeaponBones::default()
            },
            WeaponBones {
                fire_fx: Some("Fire".to_owned()),
                ..WeaponBones::default()
            },
            WeaponBones::default(),
        ],
    };
    let tank_draw = DrawModule {
        turret: Some("Turret".to_owned()),
        turret_pitch: Some("Barrel".to_owned()),
        rounds_shown: [true, false, true],
        recoil: IniRecoil {
            speed: Some(120.0),
            most: 3.0,
            damping: 0.5,
        },
        ..model("tank", tank_look)
    };
    let tank = [tank_draw, DrawModule::new(DrawKind::Other)];
    assert_eq!(draws_of(&ini, "Tank"), tank);
    // A reskin of no draw module keeps its base's; one of its own keeps none of them, and its
    // `ShowSubObject = None` empties the list before `Hatch`.
    assert_eq!(draws_of(&ini, "TankCamo"), tank);
    let red = StateLook {
        hide_show: vec![change("Hatch", false)],
        ..StateLook::default()
    };
    assert_eq!(draws_of(&ini, "TankRed"), [model("tank", red)]);
    // The tree names its model by `ModelName`, `PTOak01` read in lowercase, as every model's
    // name is; the stump's `NONE` is no model; a first state of no flag is the default.
    let tree = DrawModule {
        model: Some("ptoak01".to_owned()),
        ..DrawModule::new(DrawKind::Tree)
    };
    assert_eq!(draws_of(&ini, "Tree"), [tree]);
    let stump = draws_of(&ini, "Stump");
    assert_eq!((stump[0].kind, &stump[0].model), (DrawKind::Prop, &None));
    assert_eq!(
        draws_of(&ini, "Rock"),
        [model("rock01", StateLook::default())]
    );
    // A name finds its object of its case alone, as `findTemplate` does.
    assert_eq!(
        ini.named("Tank").map(|tank| tank.name.as_str()),
        Some("Tank")
    );
    assert!(ini.named("tank").is_none());
    // A name read before, in this file or an earlier one, and a reskin whose base is not yet
    // read, are refused at their line.
    let mut again = ObjectFiles::default();
    again.read(b"Object Rock\nEnd\n").unwrap();
    assert_eq!(
        again.read(b"Object Sand\nEnd\nObjectReskin Rock Sand\nEnd\n"),
        Err(IniError::Redefined { line: 3 })
    );
    assert_eq!(
        again.read(b"ObjectReskin Gravel Pebble\nEnd\nObject Pebble\nEnd\n"),
        Err(IniError::NoBase { line: 1 })
    );
}

#[test]
fn kinds_fences_and_scales_place_objects_as_a_multiplayer_game_does() {
    let mut ini = ObjectFiles::default();
    ini.read(PLACED.as_bytes()).unwrap();
    let placed = |name: &str| ini.named(name).unwrap().placed_as();
    let scale = |name: &str| ini.named(name).unwrap().scale().get();
    // A tree, a prop, and fluff: a build clears it, and its fence is 0 wide, or none; a fence
    // a build clears, and an object of neither kind, are units.
    assert_eq!(placed("Oak"), PlacedAs::Tree);
    assert_eq!(placed("Bench"), PlacedAs::Prop);
    assert_eq!(placed("Bush"), PlacedAs::Prop);
    assert_eq!(placed("Gravel"), PlacedAs::Prop);
    assert_eq!(placed("Fence"), PlacedAs::Unit);
    assert_eq!(placed("Tank"), PlacedAs::Unit);
    // A reskin keeps its base's kinds and scale, whatever its own lines give, and its own
    // fence's width, or its base's: a fence of width 0 is fluff, a fence's reskin a fence.
    assert_eq!(placed("BenchRed"), PlacedAs::Prop);
    assert_eq!(placed("FenceDown"), PlacedAs::Prop);
    assert_eq!(placed("FenceRed"), PlacedAs::Unit);
    assert_eq!((scale("Bench"), scale("BenchRed")), (1.5, 1.5));
    assert_eq!(scale("Oak"), 1.0);
    // A plain list clears the kinds before it sets its own; `+` and `-` keep the others;
    // `NONE` clears them all, and ends the list.
    assert_eq!(placed("Cleared"), PlacedAs::Unit);
    assert_eq!(placed("Added"), PlacedAs::Tree);
    assert_eq!(placed("Removed"), PlacedAs::Unit);
    assert_eq!(placed("Emptied"), PlacedAs::Unit);
    // A landmark bridge and a wall a unit walks on are bridge-like, whatever else they are;
    // a bridge's reskin is one, as its table cannot say otherwise.
    assert_eq!(placed("Bridge"), PlacedAs::Bridge);
    assert_eq!(placed("BridgeRed"), PlacedAs::Bridge);
    assert_eq!(placed("Wall"), PlacedAs::Bridge);
    assert_eq!(placed("Crossing"), PlacedAs::Unit);
    // A list mixes no forms, and `NONE` follows none; a scale is positive.
    let read = |text: &str| ObjectFiles::default().read(text.as_bytes());
    for kinds in [
        "PROP +SHRUBBERY",
        "+PROP SHRUBBERY",
        "PROP NONE",
        "+PROP NONE",
    ] {
        let text = format!("Object A\n KindOf = {kinds}\nEnd\n");
        assert_eq!(
            read(&text),
            Err(IniError::MixedFlags { line: 2 }),
            "{kinds}"
        );
    }
    for value in ["0.0", "-1.0", "inf"] {
        let text = format!("Object A\n Scale = {value}\nEnd\n");
        assert_eq!(read(&text), Err(IniError::Scale { line: 2 }), "{value}");
    }
    assert!(matches!(
        read("Object A\n FenceWidth = wide\nEnd\n"),
        Err(IniError::Real { line: 2, .. })
    ));
}

#[test]
fn a_file_the_game_cannot_read_is_refused_at_its_line() {
    let read = |text: &str| ObjectFiles::default().read(text.as_bytes());
    assert_eq!(
        read("Object A\n  Draw = W3DModelDraw M\n"),
        Err(IniError::Unclosed)
    );
    assert_eq!(
        read("Object A\nEnd\nEnd\n"),
        Err(IniError::StrayEnd { line: 3 })
    );
    assert_eq!(read("Weapon A\nEnd\n"), Err(IniError::NotBlock { line: 1 }));
    assert_eq!(read("Object\nEnd\n"), Err(IniError::NoName { line: 1 }));
    assert_eq!(
        read("ObjectReskin A\nEnd\n"),
        Err(IniError::NoName { line: 1 })
    );
    let state =
        |line: &str| format!("Object A\n Draw = W3DModelDraw M\n  {line}\n  End\n End\nEnd\n");
    assert_eq!(
        read(&state("ConditionState = DAMAGED")),
        Err(IniError::NoDefaultState { line: 3 })
    );
    // A default state of no model; one after another state.
    assert_eq!(
        read(&state("ConditionState NONE")),
        Err(IniError::NoModel { line: 4 })
    );
    let late = "Object A\n Draw = W3DModelDraw M\n  ConditionState\n   Model = X\n  End\n  DefaultConditionState\n";
    assert_eq!(read(late), Err(IniError::LateDefaultState { line: 6 }));
    let field = |line: &str| {
        format!(
            "Object A\n Draw = W3DModelDraw M\n  DefaultConditionState\n   {line}\n  End\n End\nEnd\n"
        )
    };
    assert_eq!(
        read(&field("WeaponMuzzleFlash = QUATERNARY Flash")),
        Err(IniError::WeaponSlot { line: 4 })
    );
    assert_eq!(
        read(&field("WeaponLaunchBone = PRIMARY")),
        Err(IniError::WeaponSlot { line: 4 })
    );
    // `ArmorSet` alone opens a block: after `;`, which starts a comment, it is alone and needs
    // its `End`; before `//`, which the game does not strip, it is a field.
    assert_eq!(read("Object A\n ArmorSet ; note\n End\nEnd\n"), Ok(()));
    assert_eq!(
        read("Object A\n ArmorSet ; note\nEnd\n"),
        Err(IniError::Unclosed)
    );
    assert_eq!(read("Object A\n ArmorSet // note\nEnd\n"), Ok(()));
}

#[test]
fn a_geometry_bounds_its_object_as_the_game_computes() {
    // A box of radii 12 and 9 and height 40: a circle of √(144 + 81) = 15, a sphere of
    // √(144 + 81 + 400) = 25. A cylinder of radius 3 and height 10 reaches its half height, 5;
    // one of radius 6 its radius. A sphere, and an object of no geometry, the game's sphere of
    // radius 1, reach their radius.
    let mut ini = ObjectFiles::default();
    let objects = "Object Box\n Geometry = BOX\n GeometryMajorRadius = 12\n GeometryMinorRadius = 9\n GeometryHeight = 40\nEnd\n\
        Object Tall\n Geometry = CYLINDER\n GeometryMajorRadius = 3\n GeometryHeight = 10\nEnd\n\
        Object Wide\n Geometry = CYLINDER\n GeometryMajorRadius = 6\n GeometryHeight = 10\nEnd\n\
        Object Ball\n Geometry = SPHERE\n GeometryMajorRadius = 7\nEnd\nObject Plain\nEnd\n";
    ini.read(objects.as_bytes()).unwrap();
    let ini = ini.finish();
    let bounds = |name: &str| {
        let geometry = ini.object(name.as_bytes()).unwrap().geometry();
        [geometry.bounding_circle(), geometry.bounding_sphere()]
    };
    assert_eq!(bounds("Box"), [15.0, 25.0]);
    assert_eq!(bounds("Tall"), [3.0, 5.0]);
    assert_eq!(bounds("Wide"), [6.0, 6.0]);
    assert_eq!(bounds("Ball"), [7.0, 7.0]);
    assert_eq!(bounds("Plain"), [1.0, 1.0]);
}

#[test]
fn a_model_draws_the_state_that_best_matches_its_maps_conditions() {
    let mut ini = ObjectFiles::default();
    let objects = "Object Barracks
  Draw = W3DModelDraw ModuleTag_01
    DefaultConditionState
      Model = DAY
      HideSubObject = Flag
      Animation = Day_SKL.Day_Idle 2.5 2
      AnimationMode = LOOP
      Flags = START_FRAME_LAST MAINTAIN_FRAME_ACROSS_STATES2
      AnimationSpeedFactorRange = 0.5 1.5
      TransitionKey = TRANS_A
    End
    ConditionState = NIGHT
      Model = NIGHT
      IdleAnimation = DAY_SKL.DAY_IDLE2
      AnimationMode = ONCE
      TransitionKey = TRANS_B
      WaitForStateToFinishIfPossible = TRANS_A
    End
    TransitionState = TRANS_A TRANS_B
      Animation = Day_SKL.Day_Old
      AnimationMode = ONCE
    End
    TransitionState = TRANS_A TRANS_B
      Animation = Day_SKL.Day_Turn
      AnimationMode = ONCE
    End
    AliasConditionState = NIGHT DAMAGED
    ConditionState = SNOW
      Model = SNOW
    End
    ConditionState = NIGHT SNOW
      Model = NIGHTSNOW
    End
    ConditionState = DAMAGED
      Model = HURT
    End
  End
End
Object Lamp
  Draw = W3DModelDraw ModuleTag_01
    ConditionState = NONE
      Model = LAMP
    End
    AliasConditionState = SNOW
    ConditionState = NIGHT
      Model = LAMPLIT
    End
    IgnoreConditionStates = NIGHT
  End
End
";
    ini.read(objects.as_bytes()).unwrap();
    let at = |night, snow| LookConditions { night, snow };
    let drawn = |name: &str, conditions| {
        let draw = &ini.named(name).unwrap().draws()[0];
        let state = draw.drawn(conditions);
        (state.model.map(str::to_owned), state.look.clone())
    };
    // A day map draws the default. At night the night state, of one flag in common and none the
    // map lacks, before its alias with damage, of one and one; it starts as the default's copy,
    // so it hides the flag too. Snow the snow state; both the state of both, of two in common.
    let flag = StateLook {
        hide_show: vec![HideShow {
            name: "Flag".to_owned(),
            hide: true,
        }],
        ..StateLook::default()
    };
    let barracks = |model: &str| (Some(model.to_owned()), flag.clone());
    assert_eq!(drawn("Barracks", at(false, false)), barracks("day"));
    assert_eq!(drawn("Barracks", at(true, false)), barracks("night"));
    assert_eq!(drawn("Barracks", at(false, true)), barracks("snow"));
    assert_eq!(drawn("Barracks", at(true, true)), barracks("nightsnow"));
    // A lamp ignores the night, so it draws its default at night; its default's alias holds
    // snow, so it draws its default in snow too.
    let lamp = (Some("lamp".to_owned()), StateLook::default());
    for conditions in [at(true, false), at(false, true), at(true, true)] {
        assert_eq!(drawn("Lamp", conditions), lamp, "{conditions:?}");
    }
    // The default's animation: its clip twice, as its line repeats it, of a stride of 2.5,
    // looped from its last frame, its frame kept across the second group, at a speed of 0.5 to
    // 1.5, of its key.
    let module = &ini.named("Barracks").unwrap().draws()[0];
    let states: Vec<&IniAnimation> = module.states().map(|state| state.animation).collect();
    let clip = |name: &str, distance, idle| IniClip {
        name: name.to_owned(),
        distance,
        idle,
    };
    let idle = clip("day_skl.day_idle", 2.5, false);
    let default = states[0];
    assert_eq!(default.clips, [idle.clone(), idle]);
    assert_eq!(default.mode, IniMode::Loop);
    let flags = [
        AnimationFlag::StartFrameLast,
        AnimationFlag::MaintainFrame2,
        AnimationFlag::RandomStart,
    ];
    assert_eq!(
        flags.map(|flag| default.flags.holds(flag)),
        [true, true, false]
    );
    assert_eq!(default.speed, [0.5, 1.5]);
    assert_eq!(
        (default.key.as_deref(), default.waits_for.as_deref()),
        (Some("trans_a"), None)
    );
    // The night's: the default's copy, its clips replaced by its own idle clip at its first
    // clip line, once, of its own keys; the snow's, the default's copy untouched.
    let night = states[1];
    assert_eq!(night.clips, [clip("day_skl.day_idle2", 0.0, true)]);
    assert_eq!(
        (night.mode, night.flags, night.speed),
        (IniMode::Once, default.flags, default.speed)
    );
    assert_eq!(
        (night.key.as_deref(), night.waits_for.as_deref()),
        (Some("trans_b"), Some("trans_a"))
    );
    let snow = states[2];
    assert_eq!(
        (&snow.clips, snow.mode, &snow.key),
        (&default.clips, default.mode, &default.key)
    );
    // The transition: the later of its pair's two, the default's copy, of its model and hidden
    // flag, of its own clip once and no key; its alias after it is the night's.
    assert_eq!(module.transitions().len(), 1);
    let transition = &module.transitions()[0];
    assert_eq!(
        (transition.from.as_str(), transition.to.as_str()),
        ("trans_a", "trans_b")
    );
    assert_eq!(transition.drawn().model, Some("day"));
    assert_eq!(transition.drawn().look, &flag);
    let turn = &transition.animation;
    assert_eq!(turn.clips, [clip("day_skl.day_turn", 0.0, false)]);
    assert_eq!((turn.mode, &turn.key), (IniMode::Once, &None));
    assert_eq!(module.states[0].sets.len(), 2);

    // Idle clips of a loop, or beside other clips, an idle line of none too; a transition of a loop, from a key to itself,
    // or of no model with no default; a mode or a flag the game does not name.
    let fails = |state: &str| {
        let text = format!(
            "Object A\n Draw = W3DModelDraw M\n  ConditionState = NONE\n   Model = A\n  End\n{state}\n End\nEnd\n"
        );
        ObjectFiles::default().read(text.as_bytes()).unwrap_err()
    };
    let state = |fields: &str| format!("  ConditionState = NIGHT\n   Model = A\n{fields}  End");
    assert_eq!(
        fails(&state("   IdleAnimation = A.B\n   AnimationMode = LOOP\n")),
        IniError::Animation { line: 10 }
    );
    assert_eq!(
        fails(&state(
            "   IdleAnimation = A.B\n   Animation = A.C\n   AnimationMode = ONCE\n"
        )),
        IniError::Animation { line: 11 }
    );
    assert_eq!(
        fails(&state(
            "   IdleAnimation = None\n   Animation = A.C\n   AnimationMode = ONCE\n"
        )),
        IniError::Animation { line: 11 }
    );
    let transition =
        |head: &str, fields: &str| format!("  TransitionState = {head}\n{fields}  End");
    assert_eq!(
        fails(&transition(
            "A B",
            "   Model = A\n   Animation = A.B\n   AnimationMode = LOOP\n"
        )),
        IniError::Animation { line: 10 }
    );
    assert_eq!(
        fails(&transition("A a", "   Model = A\n")),
        IniError::Transition { line: 6 }
    );
    assert_eq!(
        fails(&transition("A", "   Model = A\n")),
        IniError::Transition { line: 6 }
    );
    assert_eq!(fails(&transition("A B", "")), IniError::NoModel { line: 7 });
    assert_eq!(
        fails(&state("   AnimationMode = SIDEWAYS\n")),
        IniError::NotListed { line: 8 }
    );
    assert!(matches!(
        fails(&state("   Flags = SPIN\n")),
        IniError::NotListed { line: 8 }
    ));

    // A later state of a module with no `DefaultConditionState` copies nothing, so it must name
    // its model; an alias needs a state before it.
    let fails = |text: &str| ObjectFiles::default().read(text.as_bytes()).unwrap_err();
    let unnamed = "Object A\n Draw = W3DModelDraw M\n  ConditionState = NONE\n   Model = A\n  End\n  \
                   ConditionState = NIGHT\n  End\n End\nEnd\n";
    assert_eq!(fails(unnamed), IniError::NoModel { line: 7 });
    let first = "Object A\n Draw = W3DModelDraw M\n  AliasConditionState = NIGHT\n End\nEnd\n";
    assert_eq!(fails(first), IniError::FirstAlias { line: 3 });
}

#[test]
fn an_object_gives_supply_its_dock_center_gatherer_and_one_shot_spawns() {
    let text = "Object Center\n\
                 Behavior = SupplyCenterDockUpdate M1\n  NumberApproachPositions = -1\n End\n\
                 Behavior = SupplyCenterProductionExitUpdate M2\n  UnitCreatePoint = X:1 Y:2 Z:0\n\
                   NaturalRallyPoint = X:3 Y:4 Z:0\n End\n\
                 Behavior = SpawnBehavior M3\n  SpawnNumber = 4\n  SpawnTemplateName = Truck\n\
                   SpawnTemplateName = Worker Other\n  OneShot = Yes\n End\n\
                 Behavior = SpawnBehavior M4\n  SpawnNumber = 2\n  SpawnTemplateName = Guard\n End\n\
                End\n\
                Object Dock\n Behavior = SupplyWarehouseDockUpdate M1\n  StartingBoxes = 400\n\
                  DeleteWhenEmpty = Yes\n End\nEnd\n\
                Object Pile\n Behavior = SupplyWarehouseDockUpdate M1\n End\nEnd\n\
                Object Truck\n Behavior = SupplyTruckAIUpdate M1\n  MaxBoxes = 4\n\
                  SupplyCenterActionDelay = 400\n  SupplyWarehouseActionDelay = 1000\n\
                  SupplyWarehouseScanDistance = 700\n  Turret\n   TurretTurnRate = 60\n  End\n\
                 End\nEnd\n\
                Object Worker\n Behavior = WorkerAIUpdate M1\n  MaxBoxes = 1\n End\nEnd\n\
                Object Zone\n Behavior = OCLUpdate M1\n  OCL = OCL_CargoPlane\n End\nEnd\n\
                Object Mob\n Behavior = SpawnBehavior M1\n  SpawnNumber = 10\n\
                  SpawnReplaceDelay = 30000\n  SpawnTemplateName = Guard\n\
                  AggregateHealth = Yes\n End\nEnd\n\
                Object Rabble\n Behavior = SpawnBehavior M1\n  SpawnNumber = 10\n\
                  SpawnTemplateName = Guard\n  AggregateHealth = Yes\n\
                  SlavesHaveFreeWill = Yes\n End\nEnd\n";
    let mut ini = ObjectFiles::default();
    ini.read(text.as_bytes()).unwrap();
    let supply = |name: &str| ini.named(name).unwrap().supply().clone();
    // The center takes boxes, and its exit starts its gatherers. Its one-shot spawn makes 4,
    // stepping through its three names and back to the first; its other spawn, not one-shot,
    // none as it is made.
    let center = supply("Center");
    assert_eq!(
        (center.center, center.dock, center.gatherer),
        (true, None, None)
    );
    let spawned: Vec<&str> = center.one_shots().collect();
    assert_eq!(spawned, ["Truck", "Worker", "Other", "Truck"]);
    assert_eq!(
        ini.named("Center").unwrap().build().exit,
        Some(ProductionExit {
            create: [1.0, 2.0],
            rally: [3.0, 4.0],
            starts_gatherers: true,
        })
    );
    // The center's other spawn keeps 2 guards, and leads none: its health is its own, as
    // `SpawnBehaviorModuleData` sets `AggregateHealth` and `SlavesHaveFreeWill` false. The mob's
    // health is its 10 guards', and they act on its orders alone, so it leads them; the rabble's
    // guards act on their own, so it leads none (09-zero-hour.md, The import converts a mob).
    let kept = |name: &str| supply(name).kept().cloned().unwrap();
    let guards = |number, replace_ms, aggregate_health, free_will| IniSpawn {
        number,
        one_shot: false,
        replace_ms,
        templates: vec!["Guard".to_owned()],
        aggregate_health,
        free_will,
    };
    assert_eq!(kept("Center"), guards(2, 0, false, false));
    assert_eq!(kept("Mob"), guards(10, 30_000, true, false));
    assert_eq!(kept("Rabble"), guards(10, 0, true, true));
    let leads = ["Center", "Mob", "Rabble"].map(|name| kept(name).leads());
    assert_eq!(leads, [false, true, false]);
    // A dock of its boxes, which goes when empty; one of no field holds 1 box and stays.
    let dock = |boxes, delete_when_empty| {
        Some(SupplyDock {
            boxes,
            delete_when_empty,
        })
    };
    assert_eq!(supply("Dock").dock, dock(400, true));
    assert_eq!(supply("Pile").dock, dock(1, false));
    assert!(!supply("Dock").center);
    // A truck's numbers; a worker's past its boxes the game's defaults: no delay, 100 of scan.
    let gatherer = |max_boxes, center_delay_ms, warehouse_delay_ms, scan_distance| {
        Some(SupplyGatherer {
            max_boxes,
            center_delay_ms,
            warehouse_delay_ms,
            scan_distance,
        })
    };
    assert_eq!(supply("Truck").gatherer, gatherer(4, 400, 1000, 700.0));
    assert_eq!(supply("Worker").gatherer, gatherer(1, 0, 0, 100.0));
    assert_eq!(supply("Truck").one_shots().count(), 0);
    // An `OCLUpdate`'s objects come over time; no other object has one.
    assert!(supply("Zone").creates_over_time);
    assert!(!supply("Center").creates_over_time);
    // A gatherer's module is an AI module, whose turret combat reads too.
    let turret = ini.named("Truck").unwrap().fight().turret;
    assert_eq!(turret.map(|turret| turret.turn_rate), Some(60.0));
}

#[test]
fn an_object_gives_production_its_cost_time_set_prerequisites_queue_and_exit() {
    fn build(ini: &ObjectFiles, name: &str) -> IniBuild {
        ini.named(name).unwrap().build().clone()
    }
    let mut ini = ObjectFiles::default();
    ini.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
    ini.read(OBJECTS.as_bytes()).unwrap();
    // The command center: its fields, a `ProductionUpdate` of the default queue of 9, its exit's
    // points of `X:` and `Y:`, the height dropped, and the angle its placement shows at.
    assert_eq!(
        build(&ini, "AmericaCommandCenter"),
        IniBuild {
            cost: 2000,
            time: 45.0,
            command_set: Some("AmericaCommandCenterCommandSet".to_owned()),
            prerequisites: Vec::new(),
            queue: Some(9),
            exit: Some(ProductionExit {
                create: [-18.0, 35.0],
                rally: [60.0, 35.0],
                starts_gatherers: false,
            }),
            placement_angle: Some(Num::int(-135)),
            variations: Vec::new(),
        }
    );
    // The barracks' queue of 5, and its points, of a `QueueProductionExitUpdate`, which names them
    // as the default exit does; the ranger's line of one object; the pathfinder's of two; the
    // power plant's science. An object of no fields is free, a second to build, and of no
    // module.
    assert_eq!(build(&ini, "AmericaBarracks").queue, Some(5));
    assert_eq!(
        build(&ini, "AmericaBarracks").exit,
        Some(ProductionExit {
            create: [0.0, 0.0],
            rally: [55.0, 0.0],
            starts_gatherers: false,
        })
    );
    let line =
        |names: &[&str]| Prerequisite::Objects(names.iter().map(|&name| name.to_owned()).collect());
    assert_eq!(
        build(&ini, "AmericaInfantryRanger").prerequisites,
        [line(&["AmericaBarracks"])]
    );
    assert_eq!(
        build(&ini, "AmericaInfantryPathfinder").prerequisites,
        [line(&["AmericaBarracks", "AmericaCommandCenter"])]
    );
    assert_eq!(
        build(&ini, "AmericaPowerPlant").prerequisites,
        [Prerequisite::Science("SCIENCE_AMERICA".to_owned())]
    );
    assert_eq!(build(&ini, "AmericaFireBase"), IniBuild::default());
    // A reskin keeps its base's, as its table sets none of them; a second `Prerequisites` block
    // starts the list again.
    ini.read(
        b"ObjectReskin CenterRed AmericaCommandCenter\n BuildCost = 1\n CommandSet = Other\nEnd\n\
          Object Twice\n Prerequisites\n  Object = A\n End\n Prerequisites\n  Object = B\n End\nEnd\n",
    )
    .unwrap();
    assert_eq!(
        build(&ini, "CenterRed"),
        build(&ini, "AmericaCommandCenter")
    );
    assert_eq!(build(&ini, "Twice").prerequisites, [line(&["B"])]);
    // A cost past an unsigned short, and a point that lacks a label, are refused.
    let fails = |text: &str| ObjectFiles::default().read(text.as_bytes()).unwrap_err();
    assert_eq!(
        fails("Object A\n PlacementViewAngle = 1e40\nEnd\n"),
        IniError::Range { line: 2 }
    );
    assert_eq!(
        fails("Object A\n BuildCost = 65536\nEnd\n"),
        IniError::Range { line: 2 }
    );
    assert_eq!(
        fails(
            "Object A\n Behavior = DefaultProductionExitUpdate M\n  UnitCreatePoint = 1 2 3\n End\nEnd\n"
        ),
        IniError::Coord { line: 3 }
    );
}

/// A shell, of its curve's fields, a life of its own and a field the import does not read, and a
/// tank whose AI module holds a turret and a second one, its `AltTurret`, after a module of a
/// block named `Turret` that is no AI's.
const FIGHTERS: &str = "Object Shell
  Behavior = DumbProjectileBehavior ModuleTag_01
    FirstHeight = 10
    SecondHeight = 2.5
    FirstPercentIndent = 50%
    SecondPercentIndent = 90%
    FlightPathAdjustDistPerSecond = 100
    MaxLifespan = 5000
    TumbleRandomly = Yes
  End
End
Object Gun
  Behavior = SlowDeathBehavior ModuleTag_01
    Turret
      TurretTurnRate = 1
    End
  End
  Behavior = AIUpdateInterface ModuleTag_02
    Turret
      TurretTurnRate = 60
      ControlledWeaponSlots = PRIMARY TERTIARY
      AllowsPitch = Yes
      TurretPitchRate = 30
      FirePitch = 70
      MinPhysicalPitch = -20
      NaturalTurretPitch = 45
    End
    AltTurret
      TurretTurnRate = 90
      ControlledWeaponSlots = SECONDARY
    End
  End
End
Object Plain
  Behavior = DumbProjectileBehavior ModuleTag_01
  End
  Behavior = AIUpdateInterface ModuleTag_02
    Turret
    End
  End
End
";

#[test]
fn an_object_gives_combat_its_shells_curve_and_its_first_turret() {
    let mut ini = ObjectFiles::default();
    ini.read(FIGHTERS.as_bytes()).unwrap();
    let fight = |name: &str| ini.named(name).unwrap().fight().clone();
    // A percent is a fraction of 1: 50% is 0.5, and 90% is 90 ÷ 100 in the game's `f32`.
    assert_eq!(
        fight("Shell").shell,
        Some(IniShell {
            first_height: 10.0,
            second_height: 2.5,
            first_indent: 0.5,
            second_indent: 90.0 / 100.0,
            follow: 100.0,
            lifespan_ms: 5000,
        })
    );
    // The AI module's first turret, of its slots and its pitch; the death module's block and
    // the second turret are none of it.
    assert_eq!(
        fight("Gun").turret,
        Some(IniTurret {
            turn_rate: 60.0,
            slots: [true, false, true],
            allows_pitch: true,
            pitch: IniTurretPitch {
                rate: 30.0,
                fire: 70.0,
                least: -20.0,
                rest: 45.0,
            },
        })
    );
    assert_eq!(fight("Gun").shell, None);
    // Of no field, a shell lives 10 s on a straight curve, and a turret turns and pitches 0.01
    // radians a frame, 0.01 × 30 ÷ (π ÷ 180) a second, each step in `f32`: 0.3 ÷ 0.017453292 =
    // 17.188732147216797, of the bits 0x41898286, with no weapon; it does not pitch, its pitches
    // level.
    let plain = fight("Plain");
    assert_eq!(plain.shell, Some(IniShell::DEFAULT));
    assert_eq!(plain.shell.unwrap().lifespan_ms, 10_000);
    let turret = plain.turret.unwrap();
    assert_eq!(turret.slots, [false; 3]);
    assert_eq!(turret.turn_rate.to_bits(), 0x4189_8286);
    assert_eq!(turret.pitch.rate.to_bits(), 0x4189_8286);
    assert!(!turret.allows_pitch);
    assert_eq!(
        [turret.pitch.fire, turret.pitch.least, turret.pitch.rest],
        [0.0; 3]
    );
    // A slot the game does not name is refused at its line.
    let mut bad = ObjectFiles::default();
    let text = "Object A\n Behavior = AIUpdateInterface M\n  Turret\n   ControlledWeaponSlots = \
                FOURTH\n  End\n End\nEnd\n";
    assert_eq!(
        bad.read(text.as_bytes()),
        Err(IniError::WeaponSlot { line: 4 })
    );

    // The ranger starts with its set of no condition, its second, and takes its first, of
    // `PLAYER_UPGRADE`, by an upgrade; the crusader's crate set is neither, and its armor of
    // that flag its upgrade's.
    let fixture = ObjectIni::of(&[DEFAULT_OBJECTS, OBJECTS]);
    let object = |name: &str| fixture.named(name).unwrap();
    let weapons = |set: Option<&WeaponSet>| set.map(|set| set.weapons.clone());
    let named = |weapons: [Option<&str>; 3]| Some(weapons.map(|weapon| weapon.map(str::to_owned)));
    let ranger = object("AmericaInfantryRanger").fight();
    assert_eq!(
        weapons(ranger.starting_weapons()),
        named([Some("RangerRifle"), Some("Pistol"), None])
    );
    assert_eq!(
        weapons(ranger.upgrade_weapons()),
        named([Some("RangerRifle"), Some("Bolt"), Some("Flamer")])
    );
    assert_eq!(
        (ranger.upgrade_armor(), ranger.unconverted_sets()),
        (None, 0)
    );
    let crusader = object("AmericaTankCrusader").fight();
    assert_eq!(crusader.upgrade_weapons(), None);
    assert_eq!(
        (
            crusader.starting_armor(),
            crusader.upgrade_armor(),
            crusader.unconverted_sets()
        ),
        (Some("HumanArmor"), Some("StructureArmor"), 1)
    );
    // A set of that flag alone it starts with, as it has no other, is no upgrade's.
    let mut alone = ObjectFiles::default();
    alone
        .read(
            b"Object A
 ArmorSet
  Conditions = PLAYER_UPGRADE
  Armor = X
 End
End
",
        )
        .unwrap();
    let alone = alone.named("A").unwrap().fight();
    assert_eq!(
        (alone.starting_armor(), alone.upgrade_armor()),
        (Some("X"), None)
    );
    // Each upgrade module in its order, of its triggers, conflicts and effect; any other class
    // of the name's ending by its name.
    let names = |names: &[&str]| {
        names
            .iter()
            .map(|&name| name.to_owned())
            .collect::<Vec<_>>()
    };
    let module = |effect, triggers: &[&str], all, conflicts: &[&str]| IniUpgradeModule {
        effect,
        triggers: names(triggers),
        conflicts: names(conflicts),
        all,
    };
    let armor = "Upgrade_AmericaCompositeArmor";
    let life = |add, change| UpgradeEffect::MaxHealth { add, change };
    assert_eq!(
        object("AmericaTankCrusader").upgrades(),
        [
            module(UpgradeEffect::Armor, &[armor], false, &[]),
            module(
                life(100.0, HealthChange::AddCurrentToo),
                &[armor],
                false,
                &[]
            ),
            module(
                life(50.0, HealthChange::PreserveRatio),
                &[armor],
                false,
                &[]
            ),
            module(
                UpgradeEffect::Other("StealthUpgrade".to_owned()),
                &[armor],
                false,
                &[]
            ),
        ]
    );
    let flash = "Upgrade_AmericaRangerFlashBang";
    assert_eq!(
        object("AmericaInfantryRanger").upgrades(),
        [
            module(UpgradeEffect::WeaponSet, &[flash], false, &[]),
            module(
                UpgradeEffect::WeaponBonus,
                &[flash, armor],
                true,
                &["Upgrade_AmericaBarracksWalls", "Upgrade_Unknown"]
            ),
            module(UpgradeEffect::Armor, &["Upgrade_Lost"], false, &[]),
        ]
    );
    let text = "Object A\n Behavior = MaxHealthUpgrade M\n  ChangeType = HALF\n End\nEnd\n";
    assert_eq!(
        ObjectFiles::default().read(text.as_bytes()),
        Err(IniError::NotListed { line: 3 })
    );
}

#[test]
fn an_object_keeps_the_slow_death_a_normal_death_most_likely_takes() {
    // A crush's death, one of a status, one of 20 kept 1500 ms and one of 20 kept 3000 ms: the
    // two of 20 apply, the first of the tie; the veteran's alone applies to no regular unit.
    let deaths = |modules: &str| {
        let text = format!("Object A\n{modules}End\n");
        let mut files = ObjectFiles::default();
        files.read(text.as_bytes()).map(|()| {
            let object = files.objects().next().unwrap();
            object
                .slow_death()
                .map(|death| (death.delay_ms, death.weight))
        })
    };
    let module = |fields: &str| format!("  Behavior = SlowDeathBehavior M\n{fields}  End\n");
    let crushed = module("    DeathTypes = NONE +CRUSHED\n    DestructionDelay = 9000\n");
    let held = module("    RequiredStatus = SOLD\n    DestructionDelay = 8000\n");
    let first = module("    ProbabilityModifier = 20\n    DestructionDelay = 1500\n");
    let second = module("    ProbabilityModifier = 20\n    DestructionDelay = 3000\n");
    let modules = [crushed, held, first, second].concat();
    assert_eq!(deaths(&modules), Ok(Some((1500.0, 20))));
    // Of the default weight, 10, every death; a later line of death types replaces the one
    // before; none of a veteran's alone.
    assert_eq!(deaths(&module("")), Ok(Some((0.0, 10))));
    let again = module("    DeathTypes = NONE\n    DeathTypes = ALL -LASERED\n");
    assert_eq!(deaths(&again), Ok(Some((0.0, 10))));
    let veteran = module("    VeterancyLevels = NONE +VETERAN\n");
    assert_eq!(deaths(&veteran), Ok(None));
    assert_eq!(deaths(""), Ok(None));
    // A death of no name, or a name with no sign, fails at its line.
    for wrong in ["DeathTypes = ALL -MELTED", "DeathTypes = NORMAL"] {
        let text = module(&format!("    {wrong}\n"));
        assert_eq!(
            deaths(&text),
            Err(IniError::NotListed { line: 3 }),
            "{wrong}"
        );
    }

    // An `FXListDie` is no slow death; a normal death plays the list of its first that applies
    // and names one: not the crush's, nor one of `NONE`, but the blast's.
    let effect = |modules: &str| {
        let text = format!("Object A\n{modules}End\n");
        let mut files = ObjectFiles::default();
        files.read(text.as_bytes()).unwrap();
        let object = files.objects().next().unwrap();
        (
            object.death_effect().map(str::to_owned),
            object.slow_death().is_some(),
        )
    };
    let fx = |fields: &str| format!("  Behavior = FXListDie M\n{fields}  End\n");
    let modules = [
        fx("    DeathTypes = NONE +CRUSHED\n    DeathFX = FX_Crush\n"),
        fx("    DeathFX = NONE\n"),
        fx("    DeathFX = FX_Blast\n"),
        fx("    DeathFX = FX_Later\n"),
    ]
    .concat();
    assert_eq!(effect(&modules), (Some("FX_Blast".to_owned()), false));
    let modules = [
        fx("    DeathTypes = NONE +CRUSHED\n    DeathFX = FX_Crush\n"),
        fx("    DeathFX = FX_Blast\n"),
        fx("    DeathFX = FX_Later\n"),
        module(""),
    ]
    .concat();
    assert_eq!(effect(&modules), (Some("FX_Blast".to_owned()), true));
}
