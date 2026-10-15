use super::*;
use crate::zero_hour::object_ini::internals::{DEFAULT_OBJECTS, OBJECTS};

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
fn draws_of(ini: &ObjectIni, name: &str) -> Vec<DrawModule> {
    let default = |draw: &DrawModule| DrawModule {
        model: draw.model.clone(),
        look: draw.look.clone(),
        ..DrawModule::new(draw.kind)
    };
    ini.objects[name].draws().iter().map(default).collect()
}

#[test]
fn objects_give_their_default_models_and_looks_with_reskins_copied_as_read() {
    let mut ini = ObjectIni::default();
    ini.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
    ini.read(OBJECTS.as_bytes()).unwrap();
    let names: Vec<&str> = ini.objects().map(|object| object.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "AmericaCommandCenter",
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
            "TankRed",
            "Tree"
        ]
    );
    // The tank's default state: its model; its hides and shows, the second of `Barrel`
    // changing the first in its place; its weapon bones, a bone `None` none; `Turret =
    // TurretBone` is a field, and the debris draw module names no model.
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
                ..WeaponBones::default()
            },
            WeaponBones {
                fire_fx: Some("Fire".to_owned()),
                ..WeaponBones::default()
            },
            WeaponBones::default(),
        ],
    };
    let tank = [model("TANK", tank_look), DrawModule::new(DrawKind::Other)];
    assert_eq!(draws_of(&ini, "Tank"), tank);
    // A reskin of no draw module keeps its base's; one of its own keeps none of them, and its
    // `ShowSubObject = None` empties the list before `Hatch`.
    assert_eq!(draws_of(&ini, "TankCamo"), tank);
    let red = StateLook {
        hide_show: vec![change("Hatch", false)],
        ..StateLook::default()
    };
    assert_eq!(draws_of(&ini, "TankRed"), [model("TANK", red)]);
    // The tree names its model by `ModelName`; the stump's `NONE` is no model; a first state
    // of no flag is the default.
    let tree = DrawModule {
        model: Some("PTOak01".to_owned()),
        ..DrawModule::new(DrawKind::Tree)
    };
    assert_eq!(draws_of(&ini, "Tree"), [tree]);
    let stump = draws_of(&ini, "Stump");
    assert_eq!((stump[0].kind, &stump[0].model), (DrawKind::Prop, &None));
    assert_eq!(
        draws_of(&ini, "Rock"),
        [model("Rock01", StateLook::default())]
    );
    // A template finds its object of its case alone, as `findTemplate` does.
    assert_eq!(
        ini.object(b"Tank").map(|tank| tank.name.as_str()),
        Some("Tank")
    );
    assert!(ini.object(b"tank").is_none());
    // A name read before, in this file or an earlier one, and a reskin whose base is not yet
    // read, are refused at their line.
    let mut again = ObjectIni::default();
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
    let mut ini = ObjectIni::default();
    ini.read(PLACED.as_bytes()).unwrap();
    let placed = |name: &str| ini.objects[name].placed_as();
    let scale = |name: &str| ini.objects[name].scale().get();
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
    let read = |text: &str| ObjectIni::default().read(text.as_bytes());
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
    let read = |text: &str| ObjectIni::default().read(text.as_bytes());
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
    let mut ini = ObjectIni::default();
    let objects = "Object Box\n Geometry = BOX\n GeometryMajorRadius = 12\n GeometryMinorRadius = 9\n GeometryHeight = 40\nEnd\n\
        Object Tall\n Geometry = CYLINDER\n GeometryMajorRadius = 3\n GeometryHeight = 10\nEnd\n\
        Object Wide\n Geometry = CYLINDER\n GeometryMajorRadius = 6\n GeometryHeight = 10\nEnd\n\
        Object Ball\n Geometry = SPHERE\n GeometryMajorRadius = 7\nEnd\nObject Plain\nEnd\n";
    ini.read(objects.as_bytes()).unwrap();
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
    let mut ini = ObjectIni::default();
    let objects = "Object Barracks
  Draw = W3DModelDraw ModuleTag_01
    DefaultConditionState
      Model = DAY
      HideSubObject = Flag
    End
    ConditionState = NIGHT
      Model = NIGHT
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
        let draw = &ini.object(name.as_bytes()).unwrap().draws()[0];
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
    assert_eq!(drawn("Barracks", at(false, false)), barracks("DAY"));
    assert_eq!(drawn("Barracks", at(true, false)), barracks("NIGHT"));
    assert_eq!(drawn("Barracks", at(false, true)), barracks("SNOW"));
    assert_eq!(drawn("Barracks", at(true, true)), barracks("NIGHTSNOW"));
    // A lamp ignores the night, so it draws its default at night; its default's alias holds
    // snow, so it draws its default in snow too.
    let lamp = (Some("LAMP".to_owned()), StateLook::default());
    for conditions in [at(true, false), at(false, true), at(true, true)] {
        assert_eq!(drawn("Lamp", conditions), lamp, "{conditions:?}");
    }

    // A later state of a module with no `DefaultConditionState` copies nothing, so it must name
    // its model; an alias needs a state before it.
    let fails = |text: &str| ObjectIni::default().read(text.as_bytes()).unwrap_err();
    let unnamed = "Object A\n Draw = W3DModelDraw M\n  ConditionState = NONE\n   Model = A\n  End\n  \
                   ConditionState = NIGHT\n  End\n End\nEnd\n";
    assert_eq!(fails(unnamed), IniError::NoModel { line: 7 });
    let first = "Object A\n Draw = W3DModelDraw M\n  AliasConditionState = NIGHT\n End\nEnd\n";
    assert_eq!(fails(first), IniError::FirstAlias { line: 3 });
}
