use campfire_client_files::ClientUnit;
use campfire_projectiles::{CruiseData, ProjectileHits, SteerData};
use campfire_values::FilterData;

use super::*;
use crate::zero_hour::error::{NameClash, NameError};
use crate::zero_hour::format::ini::armor_ini::samples::ARMORS;
use crate::zero_hour::format::ini::locomotor_ini::samples;
use crate::zero_hour::format::ini::object_ini::samples::{DEFAULT_OBJECTS, OBJECTS};
use crate::zero_hour::format::ini::weapon_ini::WeaponIni;
use crate::zero_hour::format::ini::weapon_ini::samples::WEAPONS;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::object_flight::samples::shell_projectile;
use crate::zero_hour::rules_names::RulesNames;

/// A rocket of a `MissileAIUpdate` and no locomotor that thrusts, which the import flies not.
const ROCKET: &str = "Object Rocket\n  Behavior = MissileAIUpdate ModuleTag_01\n  End\nEnd\n";

/// The missiles of the fixture: a guided one, 2 m wide, that flies 1,250 ms, leaves at 150 u/s
/// and flies 20 u straight; a point one, as a Scud's, that flies 5 s, leaves at 50 u/s, flies
/// 200 u straight and dives within 200 u of its point; a missile that ignites late, one of no fuel
/// limit, one that flies a frame, one that bursts as its fuel ends, one that kills a garrison,
/// and one that walks.
const MISSILES: &str = "Object Missile\n  Behavior = MissileAIUpdate ModuleTag_01\n    \
    TryToFollowTarget = Yes\n    FuelLifetime = 1250\n    InitialVelocity = 150\n    \
    DistanceToTravelBeforeTurning = 20\n  End\n  Locomotor = SET_NORMAL MissileLocomotor\n  \
    Geometry = Sphere\n  GeometryMajorRadius = 1.0\nEnd\n\
    Object Scud\n  Behavior = MissileAIUpdate ModuleTag_01\n    TryToFollowTarget = No\n    \
    FuelLifetime = 5000\n    InitialVelocity = 50\n    DistanceToTravelBeforeTurning = 200\n    \
    DistanceToTargetBeforeDiving = 200\n  End\n  Locomotor = SET_NORMAL ScudLocomotor\nEnd\n\
    Object Ignited\n  Behavior = MissileAIUpdate ModuleTag_01\n    FuelLifetime = 1000\n    \
    IgnitionDelay = 30\n  End\n  Locomotor = SET_NORMAL MissileLocomotor\nEnd\n\
    Object Endless\n  Behavior = MissileAIUpdate ModuleTag_01\n  End\n  \
    Locomotor = SET_NORMAL MissileLocomotor\nEnd\n\
    Object Puff\n  Behavior = MissileAIUpdate ModuleTag_01\n    FuelLifetime = 1\n  End\n  \
    Locomotor = SET_NORMAL MissileLocomotor\nEnd\n\
    Object Burster\n  Behavior = MissileAIUpdate ModuleTag_01\n    FuelLifetime = 1000\n    \
    DetonateOnNoFuel = Yes\n  End\n  Locomotor = SET_NORMAL MissileLocomotor\nEnd\n\
    Object Flasher\n  Behavior = MissileAIUpdate ModuleTag_01\n    FuelLifetime = 1000\n    \
    GarrisonHitKillCount = 3\n  End\n  Locomotor = SET_NORMAL MissileLocomotor\nEnd\n\
    Object Walker\n  Behavior = MissileAIUpdate ModuleTag_01\n    FuelLifetime = 1000\n  End\n  \
    Locomotor = SET_NORMAL Feet\nEnd\n";

/// The missiles' locomotors: one of 225 u/s, 675 u/s² and 100 °/s, one of 200 u/s, 675 u/s² and
/// 540 °/s that holds 120 u over the ground, each thrusting, and feet.
const LOCOMOTORS: &str = "Locomotor MissileLocomotor\n  Speed = 225\n  Acceleration = 675\n  \
    TurnRate = 100\n  Appearance = THRUST\nEnd\n\
    Locomotor ScudLocomotor\n  Speed = 200\n  Acceleration = 675\n  TurnRate = 540\n  \
    PreferredHeight = 120\n  Appearance = THRUST\nEnd\n\
    Locomotor Feet\n  Speed = 20\n  Appearance = TWO_LEGS\nEnd\n";

/// The weapons of the missiles: a Tank Hunter's of a radius of 5 and a minimum range of 5; a
/// Scud's of an outer ring of 100 dealing 50 and a minimum range of 200; a torch that sprays a
/// stream; a poisoner whose missile leaves a poison field; a buggy's that scatters 10 m; one of
/// each missile that drops; one whose minimum range is its range; and clips of one shot, of no
/// delay, of a random delay, and of no reload.
const MISSILE_WEAPONS: &str = "Weapon TankHunter\n  PrimaryDamage = 40\n  \
    PrimaryDamageRadius = 5\n  AttackRange = 175\n  MinimumAttackRange = 5\n  \
    DelayBetweenShots = 1000\n  ProjectileObject = Missile\nEnd\n\
    Weapon ScudGun\n  PrimaryDamage = 300\n  PrimaryDamageRadius = 50\n  SecondaryDamage = 50\n  \
    SecondaryDamageRadius = 100\n  AttackRange = 350\n  MinimumAttackRange = 200\n  \
    DelayBetweenShots = 100\n  ProjectileObject = Scud\nEnd\n\
    Weapon BuggyRocket\n  PrimaryDamage = 20\n  ScatterRadius = 10\n  AttackRange = 300\n  \
    DelayBetweenShots = 100\n  ProjectileObject = Missile\nEnd\n\
    Weapon Torch\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 100\n  \
    ProjectileObject = Missile\n  ProjectileStreamName = FlameStream\nEnd\n\
    Weapon Poisoner\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 100\n  \
    ProjectileObject = Missile\n  ProjectileDetonationOCL = OCL_Poison\nEnd\n\
    Weapon Igniter\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 100\n  \
    ProjectileObject = Ignited\nEnd\n\
    Weapon Endless\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 100\n  \
    ProjectileObject = Endless\nEnd\n\
    Weapon Puffer\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 100\n  \
    ProjectileObject = Puff\nEnd\n\
    Weapon Burster\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 100\n  \
    ProjectileObject = Burster\nEnd\n\
    Weapon Flasher\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 100\n  \
    ProjectileObject = Flasher\nEnd\n\
    Weapon Walker\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 100\n  \
    ProjectileObject = Walker\nEnd\n\
    Weapon Close\n  PrimaryDamage = 10\n  AttackRange = 50\n  MinimumAttackRange = 50\n  \
    DelayBetweenShots = 100\nEnd\n\
    Weapon OneShot\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 0\n  \
    ClipSize = 1\n  ClipReloadTime = 2000\nEnd\n\
    Weapon RandomShot\n  PrimaryDamage = 10\n  AttackRange = 75\n  \
    DelayBetweenShots = Min:200 Max:400\n  ClipSize = 1\n  ClipReloadTime = 1000\nEnd\n\
    Weapon NoReload\n  PrimaryDamage = 10\n  AttackRange = 75\n  DelayBetweenShots = 100\n  \
    ClipSize = 1\nEnd\n";

#[test]
fn a_weapon_converts_with_its_shell_splash_scatter_pitch_and_speed_or_drops_by_its_first_cause() {
    let mut ini = WeaponIni::default();
    ini.read(WEAPONS.as_bytes()).unwrap();
    let mut rules = ZeroHourRules::default();
    for objects in [DEFAULT_OBJECTS, OBJECTS, ROCKET] {
        rules.objects.read(objects.as_bytes()).unwrap();
    }
    let of = |name: &str| {
        let id = DeclaredName::new(&format!("weapon_{}", name.to_lowercase())).unwrap();
        ImportedWeapon::of(ini.get(name).unwrap(), &rules, &id)
    };
    let weapon = |name: &str| of(name).map(|converted| converted.weapon);
    // The rifle's 100 ms are ⌈100 × 0.03⌉ = 3 frames, written back as ⌊3 × 100 / 3⌋ = 100;
    // its reload's 700 ms are 21 frames, 700.
    let own = ImportedWeapon::own;
    let decimal = |value: i64| Ranked::One(Number::Value(Scalar::Decimal(Num::int(value))));
    let range = |meters: i64| Ranked::One(RangeField::Range(ActionRange::meters(Num::int(meters))));
    let enemies = Targeting::Unit(FilterData::parse("enemies").unwrap());
    let rifle = ImportedWeapon {
        keys: ActionKeys {
            range: Some(range(100)),
            ..ActionKeys::new(enemies)
        },
        attack: AttackData {
            rate: WeaponValue::Own(own(100)),
            rate_bonus: None,
            damage: WeaponValue::Own(decimal(5)),
            damage_bonus: None,
            damage_kind: DamageType::SmallArms.kind(),
            clip: Some(ClipData {
                shots: own(3),
                reload_ms: own(700),
            }),
            splash: None,
            scatter: None,
            pitch_deg: None,
            speed: None,
            streak: None,
            pre_attack: None,
        },
        delivery: None,
    };
    assert_eq!(weapon("RangerRifle"), Ok(rifle.clone()));
    assert!(of("RangerRifle").unwrap().fired.is_none());
    // The cannon fires its shell, which deals its damage within 5 m of where it ends to
    // every relation, its own unit spared, of no `SELF`; it scatters 10 m more at infantry,
    // and reaches from 15° below to 15° above. Its 2,000 ms are 60 frames, 2,000.
    let cannon = of("TankCannon").unwrap();
    assert_eq!(
        cannon.weapon,
        ImportedWeapon {
            keys: ActionKeys {
                range: Some(range(150)),
                ..rifle.keys
            },
            attack: AttackData {
                rate: WeaponValue::Own(own(2000)),
                rate_bonus: None,
                damage: WeaponValue::Own(decimal(60)),
                damage_bonus: None,
                damage_kind: DamageType::ArmorPiercing.kind(),
                clip: None,
                splash: SplashData::new(Num::int(5), RelationSet::All, true),
                scatter: ScatterData::new(
                    Num::ZERO,
                    [(RulesNames::default().infantry, Num::int(10))].into()
                ),
                pitch_deg: PitchData::new(Num::int(-15), Num::int(15)),
                speed: None,
                streak: None,
                pre_attack: None,
            },
            delivery: None,
        }
    );
    let shell = cannon.fired.unwrap();
    assert_eq!(shell.object.name, "TankShell");
    assert_eq!(shell.projectile, shell_projectile(400));
    // A projectile of no dumb behavior and no locomotor that thrusts, and one no file defines,
    // are dropped.
    assert_eq!(weapon("Launcher"), Err(WeaponDropCause::NoFlight));
    assert_eq!(weapon("Phantom"), Err(WeaponDropCause::NoFlight));
    // A radius or a scatter of no projectile, which alone ends where it is dealt or aims at its
    // scattered point; a second radius of no projectile; a shell that detonates on shrubbery. A
    // shell's radius that spares its own kind spares only shells, which take no damage, so it
    // splashes the enemies its mask names.
    assert_eq!(weapon("Flamer"), Err(WeaponDropCause::Radius));
    assert_eq!(weapon("Shotgun"), Err(WeaponDropCause::Radius));
    assert_eq!(weapon("Gatling"), Err(WeaponDropCause::Scatter));
    assert_eq!(
        weapon("Lobber").map(|lobber| lobber.attack.splash),
        Ok(SplashData::new(Num::int(10), RelationSet::Hostiles, true))
    );
    assert_eq!(weapon("Popper"), Err(WeaponDropCause::Collides));
    // The knife's 833 ms are 25 frames, ⌈833 × 0.03⌉, so 833 ms again, once for each target as
    // its `PER_ATTACK` says.
    assert_eq!(
        weapon("Knife").map(|knife| knife.attack.pre_attack),
        Ok(Some(PreAttackData {
            ms: ImportedWeapon::own(833),
            each: PreAttackEach::Target,
        }))
    );
    // The mortar keeps its minimum range of 50 within its 300.
    assert_eq!(
        weapon("Mortar").map(|mortar| mortar.keys.min_range),
        Ok(Some(decimal(50)))
    );
    assert_eq!(weapon("Pistol"), Err(WeaponDropCause::RandomDelay));
    assert_eq!(weapon("Marker"), Err(WeaponDropCause::NoDelay));
    // A speed below the default lands the damage after the shot, at 2,400 m a second, but a
    // laser's lands at once.
    let bolt = weapon("Bolt").unwrap();
    assert_eq!(bolt.attack.speed.map(Speed::get), Some(Num::int(2400)));
    assert_eq!(
        weapon("Beam").map(|beam| (beam.attack.damage, beam.attack.speed)),
        Ok((WeaponValue::Own(decimal(30)), None))
    );
    assert_eq!(weapon("Stinger"), Err(WeaponDropCause::NotGround));
    assert_eq!(weapon("Blast"), Err(WeaponDropCause::Suicide));
    assert_eq!(weapon("Pulse"), Err(WeaponDropCause::AtSelf));
    // The minigun's 250 ms are ⌈7.5⌉ = 8 frames, 266 ms; past 1 and 5 shots in a row, its
    // stages add 2 − 1 = 1 and 3 − 1 = 2 to its rate of fire, each a modifier named for it, and
    // its coast is 2,000 ms. The spinner's fast stage changes its damage, which the engine does
    // not hold.
    let minigun = of("Minigun").unwrap();
    let stage = |shots, name: &str| StageData {
        shots,
        modifier: DeclaredName::new(name).unwrap(),
    };
    assert_eq!(minigun.weapon.attack.rate, WeaponValue::Own(own(266)));
    assert_eq!(
        minigun.weapon.attack.rate_bonus,
        Some(RulesNames::default().rate_of_fire)
    );
    assert_eq!(
        minigun.weapon.attack.streak,
        Some(StreakData {
            coast_ms: 2000,
            stages: vec![
                stage(1, "weapon_minigun_mean"),
                stage(5, "weapon_minigun_fast")
            ],
        })
    );
    let added = |value: i64| StageModifier {
        stats: BTreeMap::from([(
            RulesNames::default().rate_of_fire,
            Number::Value(Scalar::Decimal(Num::int(value))),
        )]),
    };
    let named = |name: &str| DeclaredName::new(name).unwrap();
    assert_eq!(
        minigun.modifiers,
        [
            (named("weapon_minigun_mean"), added(1)),
            (named("weapon_minigun_fast"), added(2))
        ]
    );
    assert_eq!(weapon("Spinner"), Err(WeaponDropCause::ContinuousFire));
    assert_eq!(weapon("Disabler"), Err(WeaponDropCause::NoLife));
    assert_eq!(weapon("Mender"), Err(WeaponDropCause::Healing));
}

#[test]
fn a_missile_converts_to_a_steered_projectile_with_its_rings_and_minimum_or_drops_by_its_cause() {
    let mut ini = WeaponIni::default();
    ini.read(MISSILE_WEAPONS.as_bytes()).unwrap();
    let mut rules = ZeroHourRules::default();
    for objects in [DEFAULT_OBJECTS, MISSILES] {
        rules.objects.read(objects.as_bytes()).unwrap();
    }
    rules.locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    let of = |name: &str| {
        let id = DeclaredName::new(&format!("weapon_{}", name.to_lowercase())).unwrap();
        ImportedWeapon::of(ini.get(name).unwrap(), &rules, &id)
    };
    let int = |value: i64| Num::int(value);
    let decimal = |value: i64| Ranked::One(Number::Value(Scalar::Decimal(Num::int(value))));
    // The Tank Hunter's missile: its locomotor's 225 u/s, 675 u/s² and 100 °/s, its own launch of
    // 150 u/s and straight run of 20 u, the default lock of 75 u, half that at a point, its
    // 1,250 ms of fuel, ⌈37.5⌉ =
    // 38 frames, 1,266 ms, tracking its target, 2 m wide, striking structures on its way; a splash
    // of 5 m and a minimum range of 5 m.
    let hunter = of("TankHunter").unwrap();
    let steer = SteerData::builder(int(100), 1266)
        .launch_speed(int(150))
        .acceleration(int(675))
        .straight(int(20))
        .lock(int(75))
        .point_lock(int(75) / 2)
        .build()
        .unwrap();
    let structures = vec![FilterData::parse("enemies:structure").unwrap()];
    let missile = ProjectileData::builder(int(225))
        .width(int(2))
        .hits(ProjectileHits::Units(structures.clone()))
        .steer(steer)
        .build()
        .unwrap();
    let fired = hunter.fired.unwrap();
    assert_eq!(
        (fired.object.name.as_str(), &fired.projectile),
        ("Missile", &missile)
    );
    assert_eq!(hunter.weapon.keys.min_range, Some(decimal(5)));
    assert_eq!(
        hunter.weapon.attack.splash,
        SplashData::new(int(5), RelationSet::All, true)
    );
    // The Scud's: at its point, which it locks on within half its 75 u, cruising 120 u up until
    // within 200 u,
    // 5,000 ms of fuel, 150 frames, 5,000, 2 m wide by the default geometry's radius of 1; its
    // splash of 50 m deals 50 out to 100 m; its minimum range of 200 lies within its 350.
    let scud = of("ScudGun").unwrap();
    let steer = SteerData::builder(int(540), 5000)
        .launch_speed(int(50))
        .acceleration(int(675))
        .straight(int(200))
        .lock(int(75))
        .point_lock(int(75) / 2)
        .cruise(CruiseData::new(int(120), int(200)))
        .tracks(false)
        .build()
        .unwrap();
    let point = ProjectileData::builder(int(200))
        .width(int(2))
        .hits(ProjectileHits::Units(structures))
        .steer(steer)
        .build()
        .unwrap();
    assert_eq!(scud.fired.unwrap().projectile, point);
    let ring = OuterData::new(int(100), decimal(50));
    let splash = SplashData::new(int(50), RelationSet::All, true)
        .unwrap()
        .with_outer(ring);
    assert_eq!(scud.weapon.attack.splash, splash);
    assert_eq!(scud.weapon.keys.min_range, Some(decimal(200)));
    // The buggy's missile scatters 10 m, and flies to its scattered point.
    let buggy = of("BuggyRocket").unwrap();
    let scatter = ScatterData::new(int(10), [].into());
    assert_eq!(buggy.weapon.attack.scatter, scatter);
    // Each other drops by its own cause.
    let dropped = |name: &str| of(name).map(|converted| converted.weapon).err();
    for (name, cause) in [
        ("Torch", WeaponDropCause::Stream),
        ("Poisoner", WeaponDropCause::DetonationObjects),
        ("Igniter", WeaponDropCause::Ignition),
        ("Endless", WeaponDropCause::NoFuel),
        ("Puffer", WeaponDropCause::Sprayer),
        ("Burster", WeaponDropCause::FuelBurst),
        ("Flasher", WeaponDropCause::GarrisonKill),
        ("Walker", WeaponDropCause::NoFlight),
        ("Close", WeaponDropCause::MinRange),
        ("NoReload", WeaponDropCause::NoDelay),
    ] {
        assert_eq!(dropped(name), Some(cause), "{name}");
    }
    // A clip of one shot fires each reload, its delay never counting: 2,000 ms, 60 frames, and
    // 1,000 ms, 30, of no clip.
    for (name, period) in [("OneShot", 2000), ("RandomShot", 1000)] {
        let attack = of(name).unwrap().weapon.attack;
        let rate = WeaponValue::Own(ImportedWeapon::own(period));
        assert_eq!((attack.rate, attack.clip), (rate, None), "{name}");
    }
}

#[test]
fn a_continuous_fire_converts_with_a_rate_for_each_stage_or_drops() {
    let streak = |lines: &str| {
        let mut ini = WeaponIni::default();
        let text =
            format!("Weapon W\n  PrimaryDamage = 5\n  DelayBetweenShots = 100\n{lines}End\n");
        ini.read(text.as_bytes()).unwrap();
        let id = DeclaredName::new("weapon_w").unwrap();
        ImportedWeapon::of(ini.get("W").unwrap(), &ZeroHourRules::default(), &id)
            .map(|converted| converted.weapon.attack.streak.map(|streak| streak.coast_ms))
    };
    let rates = "  WeaponBonus = CONTINUOUS_FIRE_MEAN RATE_OF_FIRE 150%\n  \
                 WeaponBonus = CONTINUOUS_FIRE_FAST RATE_OF_FIRE 200%\n";
    let fire = |one: i32, two: i32, coast: i32| {
        format!(
            "  ContinuousFireOne = {one}\n  ContinuousFireTwo = {two}\n  \
             ContinuousFireCoast = {coast}\n{rates}"
        )
    };
    // Stages of no bonus change nothing, so the weapon converts with no streak; with a rate for
    // each, its coast of 1,000 ms is 30 frames, 1,000.
    assert_eq!(streak("  ContinuousFireOne = 3\n"), Ok(None));
    assert_eq!(streak(&fire(2, 6, 1000)), Ok(Some(1000)));
    // A rate for one stage alone converts too, its other stage's modifier adding 0; its coast of
    // 100 ms is 3 frames, 100. A coast of no time the engine does not hold; shots below 0, or that
    // do not grow, are no number of it.
    let mean = "  ContinuousFireOne = 1\n  ContinuousFireTwo = 2\n  ContinuousFireCoast = 100\n  \
                WeaponBonus = CONTINUOUS_FIRE_MEAN RATE_OF_FIRE 150%\n";
    assert_eq!(streak(mean), Ok(Some(100)));
    assert_eq!(streak(&fire(2, 6, 0)), Err(WeaponDropCause::ContinuousFire));
    assert_eq!(streak(&fire(-1, 6, 1000)), Err(WeaponDropCause::BadNumber));
    assert_eq!(streak(&fire(6, 6, 1000)), Err(WeaponDropCause::BadNumber));
}

#[test]
fn a_pitch_bound_reads_within_a_quarter_turn_and_a_radius_its_relations() {
    let pitch = |lines: &str| {
        let mut ini = WeaponIni::default();
        let text =
            format!("Weapon W\n  PrimaryDamage = 5\n  DelayBetweenShots = 100\n{lines}End\n");
        ini.read(text.as_bytes()).unwrap();
        let id = DeclaredName::new("weapon_w").unwrap();
        ImportedWeapon::of(ini.get("W").unwrap(), &ZeroHourRules::default(), &id)
            .map(|converted| converted.weapon.attack.pitch_deg)
    };
    // A bound of the defaults, a turn either way, is none; one side's bound keeps the
    // other's default, which the engine's quarter turn holds; a bound of no target, or of no
    // number, drops.
    assert_eq!(pitch(""), Ok(None));
    assert_eq!(
        pitch("  MaxTargetPitch = 57\n"),
        Ok(PitchData::new(Num::int(-90), Num::int(57)))
    );
    assert_eq!(
        pitch("  MinTargetPitch = 45\n  MaxTargetPitch = 80\n"),
        Ok(PitchData::new(Num::int(45), Num::int(80)))
    );
    for bound in [
        "  MinTargetPitch = 90\n",
        "  MaxTargetPitch = -90\n",
        "  MinTargetPitch = 20\n  MaxTargetPitch = 10\n",
        "  MinTargetPitch = nan\n",
    ] {
        assert_eq!(pitch(bound), Err(WeaponDropCause::BadNumber), "{bound}");
    }
    // Each player a team of its own: all three relations are all, enemies and neutrals the
    // engine's enemies, and one alone its own set; two others, none, or a radius that spares
    // the airborne, none; one that spares a projectile's own kind, its relations.
    let affects = |allies, enemies, neutrals| RadiusAffects {
        allies,
        enemies,
        neutrals,
        ..RadiusAffects::NONE
    };
    let relations = [
        affects(true, true, true),
        affects(false, true, true),
        affects(false, true, false),
        affects(false, false, true),
        affects(true, false, false),
        affects(true, true, false),
        affects(true, false, true),
        affects(false, false, false),
        RadiusAffects {
            not_similar: true,
            ..affects(true, true, true)
        },
        RadiusAffects {
            not_airborne: true,
            ..affects(true, true, true)
        },
    ]
    .map(ImportedWeapon::relations);
    assert_eq!(
        relations,
        [
            Some(RelationSet::All),
            Some(RelationSet::Enemies),
            Some(RelationSet::Hostiles),
            Some(RelationSet::Neutrals),
            Some(RelationSet::Allies),
            None,
            None,
            None,
            Some(RelationSet::All),
            None,
        ]
    );
}

/// Two soldiers, each of a gun whose name differs from the other's only in case; the second
/// of an active body of no most life; and a mob of an immortal body and the gun, which leads its
/// spawns.
const SOLDIERS: &str = "Object Gunner
  Body = ActiveBody ModuleTag_01
    MaxHealth = 10
  End
  WeaponSet
    Conditions = None
    Weapon = PRIMARY Gun
  End
End
Object Sniper
  Body = ActiveBody ModuleTag_02
  End
  WeaponSet
    Conditions = None
    Weapon = PRIMARY GUN
  End
End
Object Mob
  Locomotor = SET_NORMAL TankTreads
  Body = ImmortalBody ModuleTag_01
    MaxHealth = 99999
  End
  WeaponSet
    Conditions = None
    Weapon = PRIMARY Gun
  End
  Behavior = SpawnBehavior ModuleTag_02
    SpawnNumber = 10
    SpawnTemplateName = Gunner
    AggregateHealth = Yes
  End
End
";

#[test]
fn weapons_of_one_name_but_case_share_an_action_only_of_the_same_numbers() {
    let import = |guns: &str| {
        let mut rules = ZeroHourRules::default();
        rules.objects.read(SOLDIERS.as_bytes()).unwrap();
        rules
            .locomotors
            .read(samples::LOCOMOTORS.as_bytes())
            .unwrap();
        rules.weapons.read(guns.as_bytes()).unwrap();
        let mut names = ImportNames::default();
        for object in ["Gunner", "Sniper", "Mob"] {
            let object = rules.objects.object(object.as_bytes()).unwrap();
            names.unit_type(object, &rules).unwrap();
        }
        CombatImport::of(&rules, &mut names.into_production().into_projectiles())
    };
    let gun = |name: &str, damage: u8| {
        format!("Weapon {name}\n  PrimaryDamage = {damage}\n  DelayBetweenShots = 100\nEnd\n")
    };
    // Of the same numbers, both soldiers take the one action; the sniper's body, of no most
    // life, gives none. The mob, which fights and lives by its spawns, takes no weapon and is no
    // unliving body (09-zero-hour.md, The import converts a mob).
    let same = import(&(gun("Gun", 5) + gun("GUN", 5).as_str())).unwrap();
    let ids: Vec<&str> = same.actions.keys().map(DeclaredName::as_str).collect();
    assert_eq!(ids, ["weapon_gun"]);
    let armed: Vec<&str> = same.weapons.keys().map(TypeName::as_str).collect();
    assert_eq!(armed, ["gunner", "sniper"]);
    assert_eq!(same.unliving, ["Sniper"]);
    // Of other numbers, the second, by the types' names, is refused.
    let other = import(&(gun("Gun", 5) + gun("GUN", 6).as_str()));
    assert!(matches!(other, Err(ZeroHourError::WeaponName(name)) if name == "GUN"));
}

#[test]
fn a_living_type_takes_its_armor_by_the_name_of_its_case() {
    let guard = |armor: &str| {
        let text = format!(
            "Object Guard\n  Body = ActiveBody ModuleTag_01\n    MaxHealth = 10\n  End\n  \
                 ArmorSet\n    Conditions = None\n    Armor = {armor}\n  End\nEnd\n"
        );
        let mut rules = ZeroHourRules::default();
        rules.objects.read(text.as_bytes()).unwrap();
        rules.armors.read(ARMORS.as_bytes()).unwrap();
        let mut names = ImportNames::default();
        let object = rules.objects.object(b"Guard").unwrap();
        names.unit_type(object, &rules).unwrap();
        let mut names = names.into_production().into_projectiles();
        CombatImport::of(&rules, &mut names).map(|import| import.armors.len())
    };
    // The fixture's two armors are written whatever the types name; an armor of another
    // case is none the game finds.
    assert_eq!(guard("HumanArmor").unwrap(), 2);
    assert!(matches!(
        guard("humanarmor"),
        Err(ZeroHourError::NoArmor { object, armor }) if object == "Guard" && armor == "humanarmor"
    ));
}

#[test]
fn a_shell_takes_its_objects_name_when_its_weapons_fire_it_alike_and_else_one_of_each() {
    let tanks = |second: &str| {
        format!(
            "Object Gunner\n  WeaponSet\n    Weapon = PRIMARY Cannon\n  End\nEnd\n\
                 Object Tower\n  WeaponSet\n    Weapon = PRIMARY {second}\n  End\nEnd\n"
        )
    };
    let gun = |name: &str, speed: u16| {
        format!(
            "Weapon {name}\n  PrimaryDamage = 60\n  DelayBetweenShots = 100\n  \
                 WeaponSpeed = {speed}\n  ProjectileObject = TankShell\n  \
                 ProjectileCollidesWith = STRUCTURES WALLS\nEnd\n"
        )
    };
    let import = |second: &str, speeds: [u16; 2], extra: &str| {
        let mut rules = ZeroHourRules::default();
        rules.objects.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
        rules.objects.read(OBJECTS.as_bytes()).unwrap();
        rules
            .objects
            .read((tanks(second) + extra).as_bytes())
            .unwrap();
        let text = gun("Cannon", speeds[0]) + gun("Howitzer", speeds[1]).as_str();
        rules.weapons.read(text.as_bytes()).unwrap();
        let mut names = ImportNames::default();
        for object in rules.objects.objects() {
            names.take(object).unwrap();
        }
        for object in ["Gunner", "Tower"] {
            let object = rules.objects.object(object.as_bytes()).unwrap();
            names.unit_type(object, &rules).unwrap();
        }
        let mut names = names.into_production().into_projectiles();
        let combat = CombatImport::of(&rules, &mut names)?;
        let deliveries: Vec<(String, String)> = combat
            .actions
            .iter()
            .map(|(id, weapon)| {
                let delivery = weapon.delivery.as_ref().unwrap();
                (
                    id.as_str().to_owned(),
                    delivery.projectile.as_str().to_owned(),
                )
            })
            .collect();
        let shells: Vec<(String, ImportedType)> = names
            .types()
            .filter(|typed| typed.name.as_str().starts_with("tankshell"))
            .map(|typed| (typed.name.as_str().to_owned(), typed.imported.clone()))
            .collect();
        Ok::<_, ZeroHourError>((deliveries, shells, combat.projectile_looks))
    };
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    let shell = |name: &str, speed| {
        (
            name.to_owned(),
            ImportedType::of_projectile(shell_projectile(speed)),
        )
    };
    // Two weapons of one speed fire one type, of the object's name, which keeps its look.
    let (deliveries, shells, looks) = import("Howitzer", [400, 400], "").unwrap();
    assert_eq!(
        deliveries,
        [
            pair("weapon_cannon", "tankshell"),
            pair("weapon_howitzer", "tankshell")
        ]
    );
    assert_eq!(shells, [shell("tankshell", 400)]);
    assert_eq!(looks, []);
    // Of two speeds, each fires a type of its own, named for it, which takes the object's
    // look.
    let (deliveries, shells, looks) = import("Howitzer", [400, 300], "").unwrap();
    assert_eq!(
        deliveries,
        [
            pair("weapon_cannon", "tankshell_cannon"),
            pair("weapon_howitzer", "tankshell_howitzer")
        ]
    );
    assert_eq!(
        shells,
        [
            shell("tankshell_cannon", 400),
            shell("tankshell_howitzer", 300)
        ]
    );
    let look = |unit_type: &str| ProjectileLook {
        unit_type: TypeName::named(unit_type),
        object: TypeName::named("tankshell"),
    };
    assert_eq!(
        looks,
        [look("tankshell_cannon"), look("tankshell_howitzer")]
    );
    let mut client = ClientUnits::default();
    let own = ClientUnit::new(Vec::new());
    client
        .units
        .insert(DeclaredName::new("tankshell").unwrap(), own);
    CombatImport {
        projectile_looks: looks,
        ..CombatImport::default()
    }
    .add_projectile_looks(&mut client);
    let named: Vec<&str> = client.units.keys().map(DeclaredName::as_str).collect();
    assert_eq!(
        named,
        ["tankshell", "tankshell_cannon", "tankshell_howitzer"]
    );
    // A name of a weapon's shell that an object took is refused.
    let clash = import("Howitzer", [400, 300], "Object TankShell_Cannon\nEnd\n");
    assert!(matches!(
        clash,
        Err(ZeroHourError::Name(NameError::Clash(NameClash { first, second })))
            if first == "TankShell_Cannon" && second == "TankShell"
    ));
}
