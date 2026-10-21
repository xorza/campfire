use campfire_client_files::ClientUnit;

use super::*;
use crate::zero_hour::error::NameClash;
use crate::zero_hour::format::ini::armor_ini::samples::ARMORS;
use crate::zero_hour::format::ini::locomotor_ini::LocomotorIni;
use crate::zero_hour::format::ini::object_ini::samples::{DEFAULT_OBJECTS, OBJECTS};
use crate::zero_hour::format::ini::weapon_ini::WeaponIni;
use crate::zero_hour::format::ini::weapon_ini::samples::WEAPONS;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::imported_type::samples::shell_projectile;

/// A rocket of no `DumbProjectileBehavior`, a missile's.
const ROCKET: &str = "Object Rocket\n  Behavior = MissileAIUpdate ModuleTag_01\n  End\nEnd\n";

#[test]
fn a_weapon_converts_with_its_shell_splash_scatter_pitch_and_speed_or_drops_by_its_first_cause() {
    let mut ini = WeaponIni::default();
    ini.read(WEAPONS.as_bytes()).unwrap();
    let mut objects = ObjectIni::default();
    objects.read(DEFAULT_OBJECTS.as_bytes()).unwrap();
    objects.read(OBJECTS.as_bytes()).unwrap();
    objects.read(ROCKET.as_bytes()).unwrap();
    let of = |name: &str| {
        let id = DeclaredName::new(&format!("weapon_{}", name.to_lowercase())).unwrap();
        ImportedWeapon::of(ini.get(name).unwrap(), &objects, &id)
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
            ..ActionKeys::new(ActionKind::Attack, enemies)
        },
        attack: AttackData {
            rate: WeaponValue::Own(own(100)),
            rate_bonus: None,
            damage: WeaponValue::Own(decimal(5)),
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
        },
    };
    assert_eq!(weapon("RangerRifle"), Ok(rifle.clone()));
    assert!(of("RangerRifle").unwrap().shell.is_none());
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
                damage_kind: DamageType::ArmorPiercing.kind(),
                clip: None,
                splash: SplashData::new(Num::int(5), RelationSet::All, true),
                scatter: ScatterData::new(
                    Num::ZERO,
                    [(DeclaredName::new(INFANTRY).unwrap(), Num::int(10))].into()
                ),
                pitch_deg: PitchData::new(Num::int(-15), Num::int(15)),
                speed: None,
                streak: None,
            },
        }
    );
    let shell = cannon.shell.unwrap();
    assert_eq!(shell.object.name, "TankShell");
    assert_eq!(shell.projectile, shell_projectile(400));
    // A projectile of no dumb behavior, a missile's, and one no file defines, are dropped.
    assert_eq!(weapon("Launcher"), Err(WeaponDrop::Missile));
    assert_eq!(weapon("Phantom"), Err(WeaponDrop::Missile));
    // A radius or a scatter of no shell, which alone ends where it is dealt or aims at its
    // scattered point; a second radius; a radius that spares its own kind; a shell that
    // detonates on shrubbery.
    assert_eq!(weapon("Flamer"), Err(WeaponDrop::Radius));
    assert_eq!(weapon("Shotgun"), Err(WeaponDrop::Radius));
    assert_eq!(weapon("Gatling"), Err(WeaponDrop::Scatter));
    assert_eq!(weapon("Lobber"), Err(WeaponDrop::Affects));
    assert_eq!(weapon("Popper"), Err(WeaponDrop::Collides));
    assert_eq!(weapon("Knife"), Err(WeaponDrop::PreAttack));
    assert_eq!(weapon("Mortar"), Err(WeaponDrop::MinRange));
    assert_eq!(weapon("Pistol"), Err(WeaponDrop::RandomDelay));
    assert_eq!(weapon("Marker"), Err(WeaponDrop::NoDelay));
    // A speed below the default lands the damage after the shot, at 2,400 m a second, but a
    // laser's lands at once.
    let bolt = weapon("Bolt").unwrap();
    assert_eq!(bolt.attack.speed.map(Speed::get), Some(Num::int(2400)));
    assert_eq!(
        weapon("Beam").map(|beam| (beam.attack.damage, beam.attack.speed)),
        Ok((WeaponValue::Own(decimal(30)), None))
    );
    assert_eq!(weapon("Stinger"), Err(WeaponDrop::NotGround));
    assert_eq!(weapon("Blast"), Err(WeaponDrop::Suicide));
    assert_eq!(weapon("Pulse"), Err(WeaponDrop::AtSelf));
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
        Some(Stat::named(RATE_OF_FIRE).unwrap())
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
            Stat::named(RATE_OF_FIRE).unwrap(),
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
    assert_eq!(weapon("Spinner"), Err(WeaponDrop::ContinuousFire));
    assert_eq!(weapon("Disabler"), Err(WeaponDrop::NoLife));
    assert_eq!(weapon("Mender"), Err(WeaponDrop::Healing));
}

#[test]
fn a_continuous_fire_converts_with_a_rate_for_each_stage_or_drops() {
    let streak = |lines: &str| {
        let mut ini = WeaponIni::default();
        let text =
            format!("Weapon W\n  PrimaryDamage = 5\n  DelayBetweenShots = 100\n{lines}End\n");
        ini.read(text.as_bytes()).unwrap();
        let id = DeclaredName::new("weapon_w").unwrap();
        ImportedWeapon::of(ini.get("W").unwrap(), &ObjectIni::default(), &id)
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
    assert_eq!(streak(&fire(2, 6, 0)), Err(WeaponDrop::ContinuousFire));
    assert_eq!(streak(&fire(-1, 6, 1000)), Err(WeaponDrop::BadNumber));
    assert_eq!(streak(&fire(6, 6, 1000)), Err(WeaponDrop::BadNumber));
}

#[test]
fn a_pitch_bound_reads_within_a_quarter_turn_and_a_radius_its_relations() {
    let pitch = |lines: &str| {
        let mut ini = WeaponIni::default();
        let text =
            format!("Weapon W\n  PrimaryDamage = 5\n  DelayBetweenShots = 100\n{lines}End\n");
        ini.read(text.as_bytes()).unwrap();
        let id = DeclaredName::new("weapon_w").unwrap();
        ImportedWeapon::of(ini.get("W").unwrap(), &ObjectIni::default(), &id)
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
        assert_eq!(pitch(bound), Err(WeaponDrop::BadNumber), "{bound}");
    }
    // Each player a team of its own: all three relations are all, enemies and neutrals the
    // engine's enemies, and one alone its own set; two others, none, or a radius that
    // spares its own kind or the airborne, none.
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
            None,
            None,
        ]
    );
}

/// Two soldiers, each of a gun whose name differs from the other's only in case; the second
/// of an active body of no most life.
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
";

#[test]
fn weapons_of_one_name_but_case_share_an_action_only_of_the_same_numbers() {
    let import = |guns: &str| {
        let mut rules = ZeroHourRules::default();
        rules.objects.read(SOLDIERS.as_bytes()).unwrap();
        rules.weapons.read(guns.as_bytes()).unwrap();
        let mut names = ImportNames::default();
        for object in ["Gunner", "Sniper"] {
            let object = rules.objects.object(object.as_bytes()).unwrap();
            names.unit_type(object, &LocomotorIni::default()).unwrap();
        }
        CombatImport::of(&rules, &mut names.into_production().into_shells())
    };
    let gun = |name: &str, damage: u8| {
        format!("Weapon {name}\n  PrimaryDamage = {damage}\n  DelayBetweenShots = 100\nEnd\n")
    };
    // Of the same numbers, both soldiers take the one action; the sniper's body, of no most
    // life, gives none.
    let same = import(&(gun("Gun", 5) + gun("GUN", 5).as_str())).unwrap();
    let ids: Vec<&str> = same.actions.keys().map(DeclaredName::as_str).collect();
    assert_eq!(ids, ["weapon_gun"]);
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
        names.unit_type(object, &LocomotorIni::default()).unwrap();
        let mut names = names.into_production().into_shells();
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
            names.unit_type(object, &LocomotorIni::default()).unwrap();
        }
        let mut names = names.into_production().into_shells();
        let combat = CombatImport::of(&rules, &mut names)?;
        let deliveries: Vec<(String, String)> = combat
            .actions
            .iter()
            .map(|(id, weapon)| {
                let delivery = weapon.keys.delivery.as_ref().unwrap();
                (
                    id.as_str().to_owned(),
                    delivery.unit_type().as_str().to_owned(),
                )
            })
            .collect();
        let shells: Vec<(String, ImportedType)> = names
            .types()
            .filter(|(name, ..)| name.as_str().starts_with("tankshell"))
            .map(|(name, _, imported)| (name.as_str().to_owned(), imported.clone()))
            .collect();
        Ok::<_, ZeroHourError>((deliveries, shells, combat.shell_looks))
    };
    let pair = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    let shell = |name: &str, speed| {
        (
            name.to_owned(),
            ImportedType::shell(shell_projectile(speed)),
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
    let look = |unit_type: &str| ShellLook {
        unit_type: DeclaredName::new(unit_type).unwrap(),
        object: DeclaredName::new("tankshell").unwrap(),
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
        shell_looks: looks,
        ..CombatImport::default()
    }
    .add_shell_looks(&mut client);
    let named: Vec<&str> = client.units.keys().map(DeclaredName::as_str).collect();
    assert_eq!(
        named,
        ["tankshell", "tankshell_cannon", "tankshell_howitzer"]
    );
    // A name of a weapon's shell that an object took is refused.
    let clash = import("Howitzer", [400, 300], "Object TankShell_Cannon\nEnd\n");
    assert!(matches!(
        clash,
        Err(ZeroHourError::NameClash(NameClash { first, second }))
            if first == "TankShell_Cannon" && second == "TankShell"
    ));
}
