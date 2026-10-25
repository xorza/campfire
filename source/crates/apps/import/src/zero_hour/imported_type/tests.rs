use campfire_codec::Toml;
use campfire_values::FilterData;

use super::*;
use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;
use crate::zero_hour::format::ini::object_ini::ObjectIni;

/// The objects of each kind the import's rule of bodies tells apart.
const BODIES: &str = "Object Tank
  KindOf = VEHICLE
  Locomotor = SET_NORMAL TankTreads
  Locomotor = SET_NORMAL_UPGRADED JetWings
  Geometry = BOX
  GeometryMajorRadius = 15.0
  GeometryMinorRadius = 10.0
End
Object Jet
  Locomotor = SET_NORMAL None JetWings
  Geometry = SPHERE
  GeometryMajorRadius = 12.0
End
Object Hover
  Locomotor = SET_NORMAL JetWings TankTreads
  Geometry = CYLINDER
  GeometryMajorRadius = 6.5
  GeometryHeight = 20.0
End
Object Barracks
  KindOf = STRUCTURE
  Geometry = BOX
  GeometryMajorRadius = 60.0
  GeometryMinorRadius = 70.0
  GeometryHeight = 49.0
End
Object Tower
  KindOf = STRUCTURE
  Geometry = CYLINDER
  GeometryMajorRadius = 8.5
End
Object Mine
  KindOf = STRUCTURE
  Geometry = BOX
  GeometryMajorRadius = 3.0
  GeometryMinorRadius = 3.0
  GeometryIsSmall = Yes
End
ObjectReskin BigMine Mine
  GeometryIsSmall = No
End
Object Shrub
  KindOf = STRUCTURE
End
Object Rock
End
Object Lost
  Locomotor = SET_NORMAL NoSuchTreads
End
Object Train
  Locomotor = SET_NORMAL TankTreads
  Geometry = BOX
  GeometryMajorRadius = 112.0
  GeometryMinorRadius = 10.0
End
Object Flat
  KindOf = STRUCTURE
  Geometry = BOX
  GeometryMajorRadius = 5.0
  GeometryMinorRadius = 0.0
End
Object Huge
  KindOf = STRUCTURE
  GeometryMajorRadius = 2048.5
End
Object Dome
  KindOf = STRUCTURE
  Geometry = SPHERE
  GeometryMajorRadius = 5.0
  GeometryHeight = 9.0
End
Object Sunk
  KindOf = STRUCTURE
  Geometry = CYLINDER
  GeometryMajorRadius = 5.0
  GeometryHeight = -1.0
End
ObjectReskin WideTank Tank
  GeometryMajorRadius = 20.0
End
Object Rig
  BuildVariations = Tank WideTank
End
Object Mob
  Locomotor = SET_NORMAL TankTreads
  Geometry = CYLINDER
  GeometryMajorRadius = 6.5
  Body = ActiveBody ModuleTag_03
    MaxHealth = 100
  End
  Behavior = AIUpdateInterface ModuleTag_01
    Turret
      TurretTurnRate = 60
      ControlledWeaponSlots = PRIMARY
    End
    AutoAcquireEnemiesWhenIdle = Yes
  End
  Behavior = SpawnBehavior ModuleTag_02
    SpawnNumber = 10
    SpawnTemplateName = Tank
    AggregateHealth = Yes
  End
End
Object Rabble
  Locomotor = SET_NORMAL TankTreads
  Geometry = CYLINDER
  GeometryMajorRadius = 6.5
  Behavior = AIUpdateInterface ModuleTag_01
    Turret
      TurretTurnRate = 60
      ControlledWeaponSlots = PRIMARY
    End
    AutoAcquireEnemiesWhenIdle = Yes
  End
  Behavior = SpawnBehavior ModuleTag_02
    SpawnNumber = 10
    SpawnTemplateName = Tank
    AggregateHealth = Yes
    SlavesHaveFreeWill = Yes
  End
End
";

#[test]
fn an_objects_type_drives_by_its_ground_locomotor_and_has_its_body_of_its_height() {
    let mut rules = ZeroHourRules {
        objects: ObjectIni::of(&[BODIES]),
        ..ZeroHourRules::default()
    };
    rules.locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    let of = |name: &str| ImportedType::of(rules.objects.object(name.as_bytes()).unwrap(), &rules);
    let int = Num::int;
    let half = |x: i64| Num::from_bits(x << (Num::FRAC_BITS - 1));
    let body = |shape: Option<BodyShape>, height| {
        let shape = shape.unwrap().raised(height).unwrap();
        Some(CollisionData { shape, layer: None })
    };
    let circle = BodyShape::circle;
    let boxed = BodyShape::box_sized;
    let drives = |speed, turn_rate, radius: Num, height, gait| ImportedType {
        life: None,
        pools: None,
        tags: None,
        stats: StatsData::from([
            (Stat::Engine(EngineStat::MoveSpeed), StatValue::flat(speed)),
            (
                Stat::Engine(EngineStat::TurnRate),
                StatValue::flat(turn_rate),
            ),
        ]),
        collision: body(circle(radius), height),
        variations: None,
        combat: None,
        turret: None,
        bounty: None,
        income: None,
        projectile: None,
        navigation: UnitNavigationData::new(gait, radius * radius),
        acquire: None,
        leads: false,
    };
    let stands = |body| ImportedType {
        life: None,
        pools: None,
        tags: None,
        stats: StatsData::default(),
        collision: body,
        variations: None,
        combat: None,
        turret: None,
        bounty: None,
        income: None,
        projectile: None,
        navigation: None,
        acquire: None,
        leads: false,
    };
    // A structure stands of the tag of its kind.
    let structure = |body| ImportedType {
        tags: Some(TypeTagsData(vec![declared("structure")])),
        ..stands(body)
    };
    // The tank drives on its treads, 30.5 m and 90° a second; a box of half extents 15 and
    // 10 bounds a circle of √(15² + 10²) = √325 = 18.02775637…, in `f32` the nearest of
    // 19 bits past the point, 9,451,736 · 2⁻¹⁹ = 18.0277557373…, exactly 302,455,552 · 2⁻²⁴
    // as a `Num`. Its geometry names no height, so it has the game's default, 1 m. A `VEHICLE`,
    // it drives through a crowd, of a weight of its radius squared, as every type that drives
    // weighs (04-capabilities/navigation.md, A unit has a weight).
    let tank = of("Tank").unwrap();
    let radius = Num::from_bits(302_455_552);
    assert_eq!(tank, drives(half(61), int(90), radius, int(1), Gait::Drive));
    assert_eq!(tank.move_speed(), Some(half(61)));
    // The jet only flies: no stats, and no body. The hover craft's first normal locomotor
    // flies, and its second drives, a cylinder's major radius 6.5, 20 m tall.
    assert_eq!(of("Jet").unwrap(), stands(None));
    assert_eq!(of("Jet").unwrap().move_speed(), None);
    assert_eq!(
        of("Hover").unwrap(),
        drives(half(61), int(90), half(13), int(20), Gait::Step)
    );
    // A train's box bounds a circle of √(112² + 10²) ≈ 112.4 m, past the original's widest
    // pathfinding footprint: its circle is that footprint's 25 m.
    assert_eq!(
        of("Train").unwrap(),
        drives(half(61), int(90), int(25), int(1), Gait::Step)
    );
    // A structure that is not small: a box of twice its radii, 120 × 140, 49 m tall; a
    // cylinder's circle of its radius 8.5; one of no geometry the game's sphere of radius 1,
    // whose height is its radius whatever its `GeometryHeight`, as `GeometryInfo::set` takes
    // it, and the sphere 5 m wide with a height of 9 m, 5 m tall.
    assert_eq!(
        of("Barracks").unwrap(),
        structure(body(boxed([int(120), int(140)]), int(49)))
    );
    assert_eq!(
        of("Tower").unwrap(),
        structure(body(circle(half(17)), int(1)))
    );
    assert_eq!(
        of("Shrub").unwrap(),
        structure(body(circle(int(1)), int(1)))
    );
    assert_eq!(of("Dome").unwrap(), structure(body(circle(int(5)), int(5))));
    // A small structure and an object of no kind block nothing; a reskin's own geometry
    // makes its mine large, a box of 6 × 6.
    assert_eq!(of("Mine").unwrap(), structure(None));
    assert_eq!(of("Rock").unwrap(), stands(None));
    // A build places the box and the cylinder, and no small structure.
    let built = ["Barracks", "Tower", "Mine"].map(|name| of(name).unwrap().has_body());
    assert_eq!(built, [true, true, false]);
    assert_eq!(
        of("BigMine").unwrap(),
        structure(body(boxed([int(6), int(6)]), int(1)))
    );
    // A locomotor no file defines refuses the type, as the game refuses it.
    assert_eq!(
        of("Lost"),
        Err(TypeError::NoLocomotor("NoSuchTreads".to_owned()))
    );
    // A box of no width, a circle past the widest body's 2,048 m, and a height below 0, a
    // unit type's load refuses, so the import refuses them first.
    assert_eq!(of("Flat"), Err(TypeError::Body));
    assert_eq!(of("Huge"), Err(TypeError::Body));
    assert_eq!(of("Sunk"), Err(TypeError::Body));
    // The rig, of the tank and its reskin of a box 20 wide, of a circle of √(20² + 10²): the
    // tank's type, of the body of each in place of its own collision, writes them as its
    // `variations`, has a body, and reaches as far as the wider
    // (04-capabilities/00-overview.md, A unit type may vary its units).
    let wide = of("WideTank").unwrap().collision;
    assert_ne!(wide, tank.collision);
    let bodies = [tank.collision.clone(), wide.clone()].map(|collision| VariationData {
        collision: collision.unwrap(),
    });
    // The mob leads its spawns: it has no life, walk, body, turret or scan of its own, its active
    // body's 100 life among them, so it stands of none. The rabble's spawns act on their own, so it drives as the hover craft, of its
    // turret and its scan (09-zero-hour.md, The import converts a mob).
    let mob = of("Mob").unwrap();
    let led = ImportedType {
        leads: true,
        ..stands(None)
    };
    assert_eq!(mob, led);
    assert!(mob.leads() && !mob.walks() && !mob.has_body());
    let rabble = of("Rabble").unwrap();
    assert!(!rabble.leads() && rabble.walks());
    assert!(rabble.turret.is_some() && rabble.acquire.is_some());
    assert_eq!(rabble.collision, body(circle(half(13)), int(1)));
    let rig = of("Rig").unwrap();
    let varied = ImportedType {
        collision: None,
        variations: Some(VariationsData(bodies.into())),
        ..tank
    };
    assert_eq!(rig, varied);
    assert!(rig.has_body());
    let wide_reach = wide.unwrap().shape.circle_radius().unwrap();
    assert_eq!(rig.reach(Num::ZERO), Some([wide_reach; 2]));
    let mut writer = SectionWriter::default();
    rig.write(&mut writer);
    let written = Toml::write(&writer.into_table());
    assert!(
        written.contains("[[variations]]") && !written.contains("[collision]"),
        "{written}"
    );
}

/// The objects of each kind of life and of turret the import tells apart.
const LIVING: &str = "Object Soldier
  KindOf = INFANTRY
  Body = ActiveBody ModuleTag_01
    MaxHealth = 180.0
    InitialHealth = 180.0
  End
  ArmorSet
    Conditions = PLAYER_UPGRADE
    Armor = ChemSuitArmor
  End
  ArmorSet
    Conditions = None
    Armor = HumanArmor
  End
End
Object Center
  KindOf = STRUCTURE MP_COUNT_FOR_VICTORY FS_BASE_DEFENSE
  BuildCost = 2000
  Body = StructureBody ModuleTag_02
    MaxHealth = 5000.0
  End
End
Object Wall
  KindOf = MP_COUNT_FOR_VICTORY
  FenceWidth = 20
  Body = ActiveBody ModuleTag_03
    MaxHealth = 10
  End
  ArmorSet
    Conditions = None
    Armor = None
  End
End
Object Gunner
  Behavior = AIUpdateInterface ModuleTag_07
    Turret
      TurretTurnRate = 60
      ControlledWeaponSlots = SECONDARY
    End
    AutoAcquireEnemiesWhenIdle = Yes ATTACK_BUILDINGS
    MoodAttackCheckRate = 250
  End
End
Object Idle
  Behavior = AIUpdateInterface ModuleTag_07
    Turret
      TurretTurnRate = 60
    End
    AutoAcquireEnemiesWhenIdle = Yes Stealthed
  End
End
Object Fixed
  Behavior = AIUpdateInterface ModuleTag_07
    Turret
      TurretTurnRate = 0
      ControlledWeaponSlots = PRIMARY
    End
    AutoAcquireEnemiesWhenIdle = No NotWhileAttacking
    MoodAttackCheckRate = 250
  End
End
Object Ghost
  BuildCost = 100
  Body = ImmortalBody ModuleTag_04
    MaxHealth = 100
  End
End
Object Hollow
  Body = ActiveBody ModuleTag_05
  End
End
ObjectReskin Veteran Soldier
  Body = ActiveBody ModuleTag_06
    MaxHealth = 9999
  End
End
Object Derrick
  Body = StructureBody ModuleTag_01
    MaxHealth = 1000
  End
  Behavior = AutoDepositUpdate ModuleTag_02
    DepositTiming = 1000
    DepositAmount = 200
    InitialCaptureBonus = 1000
  End
End
Object Display
  Body = StructureBody ModuleTag_01
    MaxHealth = 1000
  End
  Behavior = AutoDepositUpdate ModuleTag_02
    DepositTiming = 1000
    DepositAmount = 200
    ActualMoney = No
  End
End
";

#[test]
fn a_living_object_lives_by_its_body_and_takes_its_starting_armor_its_tags_and_its_turret() {
    let ini = ObjectIni::of(&[LIVING]);
    let of = |name: &str| {
        ImportedType::of(
            ini.object(name.as_bytes()).unwrap(),
            &ZeroHourRules::default(),
        )
        .unwrap()
    };
    let lives = |life: i64, armor: Option<&str>, tags: Vec<&str>| ImportedType {
        life: Some(Num::int(life)),
        pools: Some(TypePoolsData(vec![declared("life")])),
        tags: (!tags.is_empty()).then(|| TypeTagsData(tags.into_iter().map(declared).collect())),
        stats: StatsData::from([(
            Stat::named("life").unwrap(),
            StatValue::flat(Num::int(life)),
        )]),
        collision: None,
        variations: None,
        combat: Some(CombatPatch {
            armor: armor.map(|armor| DeclaredName::new(armor).unwrap()),
            ..CombatPatch::default()
        }),
        turret: None,
        bounty: None,
        income: None,
        projectile: None,
        navigation: None,
        acquire: None,
        leads: false,
    };
    // The soldier lives by its active body, 180, in the armor of its set of no condition,
    // the second it gives, by the name the mode declares, and in its set of `PLAYER_UPGRADE`'s
    // while it holds that flag's tag; a reskin keeps its base's body, as its table reads none.
    let mut soldier = lives(180, Some("humanarmor"), vec!["infantry"]);
    soldier.combat = Some(CombatPatch {
        armor: Some(declared("humanarmor")),
        armors: Some(vec![TaggedArmorData {
            when: RulesNames::default().armor_upgraded,
            armor: declared("chemsuitarmor"),
        }]),
        ..CombatPatch::default()
    });
    assert_eq!(of("Soldier"), soldier);
    assert_eq!(of("Veteran"), of("Soldier"));
    // A structure that counts for victory and defends a base lives by its structure body, of no
    // armor, and of no geometry stands as the game's sphere of radius 1, 1 m tall, of the tags
    // of the three; an object of the kind that is no structure, of an armor of `None`, counts
    // for none, and of a fence's width is a wall.
    let shape = BodyShape::circle(Num::ONE)
        .unwrap()
        .raised(Num::ONE)
        .unwrap();
    // Its build cost of 2,000 is its bounty's cost, in money.
    let center = ImportedType {
        collision: Some(CollisionData { shape, layer: None }),
        variations: None,
        bounty: Some(BountyData {
            resource: declared("money"),
            pays: Pays::Cost(2000),
        }),
        ..lives(
            5000,
            None,
            vec!["counts_for_victory", "structure", "base_defense"],
        )
    };
    assert_eq!(of("Center"), center);
    assert_eq!(of("Wall"), lives(10, None, vec!["wall"]));
    // The derrick's deposit pays 200 money each 1,000 ms, 30 frames, and 1,000 at a capture; a
    // deposit of no actual money pays nothing.
    assert_eq!(
        of("Derrick").income,
        Some(IncomeData {
            resource: declared("money"),
            pays: IncomePays::Amount(200),
            period_ms: 1000,
            capture: 1000,
        })
    );
    assert_eq!(of("Display").income, None);
    // An immortal body, and a body of no most life, give no life, no combat, and no bounty,
    // whatever their cost.
    for name in ["Ghost", "Hollow"] {
        let typed = of(name);
        assert_eq!(
            (typed.pools, typed.combat, typed.bounty),
            (None, None, None),
            "{name}"
        );
    }
    // The gunner's turret turns 60° a second; a turret of no weapon, or of no turn, is none.
    let turret = TurretPatch {
        turn_rate: Some(Num::int(60)),
        ..TurretPatch::default()
    };
    assert_eq!(of("Gunner").turret, Some(turret));
    assert_eq!((of("Idle").turret, of("Fixed").turret), (None, None));
    // The gunner scans each 250 ms, ⌈250 × 0.03⌉ = 8 frames, written as ⌊8 × 100 / 3⌋ = 266 ms,
    // for every hostile unit, as it attacks buildings; the idle turret each 2,000 ms, 60 frames,
    // by default, and for the targets of the rules data's defaults; a `No` scans for none.
    let hostiles = FilterData::parse("hostiles").unwrap();
    let scan = |ms, targets| {
        Some(AcquirePatch {
            scan_ms: Some(ms),
            targets,
        })
    };
    assert_eq!(of("Gunner").acquire, scan(266, Some(vec![hostiles])));
    assert_eq!(of("Idle").acquire, scan(2000, None));
    assert_eq!(of("Fixed").acquire, None);
}

/// `name`, a name the rules package declares.
fn declared(name: &str) -> DeclaredName {
    DeclaredName::new(name).unwrap()
}
