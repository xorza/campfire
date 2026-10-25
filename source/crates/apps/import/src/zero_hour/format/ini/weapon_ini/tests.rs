use super::*;

#[test]
fn a_weapon_reads_its_numbers_its_type_its_times_and_its_projectile() {
    let mut ini = WeaponIni::default();
    ini.read(samples::WEAPONS.as_bytes()).unwrap();
    let rifle = ini.get("RangerRifle").unwrap();
    assert_eq!(
        *rifle,
        IniWeapon {
            damage: 5.0,
            radius: 0.0,
            secondary_damage: 0.0,
            secondary_radius: 0.0,
            scatter: 0.0,
            infantry_scatter: 0.0,
            range: 100.0,
            speed: IniWeapon::INSTANT,
            pitch: [-180.0, 180.0],
            laser: false,
            anti_ground: true,
            affects: RadiusAffects::DEFAULT,
            collides: CollideMask::DEFAULT,
            at_self: false,
            continuous_fire: ContinuousFire::default(),
            upgrade: UpgradeBonus::default(),
            min_range: 0.0,
            damage_type: DamageType::SmallArms,
            delay: ShotDelay::Fixed(100),
            clip_size: 3,
            clip_reload_ms: 700,
            pre_attack_ms: 0,
            pre_attack_type: PrefireType::Shot,
            projectile: None,
            stream: false,
            detonation_objects: false,
        }
    );
    // The cannon's shell detonates on structures and walls, and its radius damages every
    // relation but its own unit; its pitch is read in degrees.
    let cannon = ini.get("TankCannon").unwrap();
    assert_eq!(cannon.projectile.as_deref(), Some("TankShell"));
    assert_eq!(cannon.pitch, [-15.0, 15.0]);
    let collides = CollideMask {
        walls: true,
        ..CollideMask::DEFAULT
    };
    assert_eq!(cannon.collides, collides);
    assert_eq!(cannon.affects, RadiusAffects::DEFAULT);
    assert_eq!(ini.get("Pistol").unwrap().delay, ShotDelay::Random);
    // A weapon that names no type is of the default; a type of none of the game's names is
    // refused, as the game's reader refuses it.
    assert_eq!(
        ini.get("Mortar").unwrap().damage_type,
        DamageType::Explosion
    );
    let mut other = WeaponIni::default();
    assert_eq!(
        other.read(b"Weapon W\n  DamageType = SPLASH\nEnd\n"),
        Err(IniError::NotListed { line: 2 })
    );
    let knife = ini.get("Knife").unwrap();
    assert_eq!(
        (knife.pre_attack_ms, knife.pre_attack_type),
        (833, PrefireType::Attack)
    );
    assert_eq!(
        other.read(b"Weapon W\n  PreAttackType = PER_TURN\nEnd\n"),
        Err(IniError::NotListed { line: 2 })
    );
    // A secondary damage, a stream's name and the objects of a detonation are read; a name of
    // `None` names no stream.
    let mut extra = WeaponIni::default();
    let text = "Weapon W\n  SecondaryDamage = 2.5\n  ProjectileStreamName = Stream\n  \
                ProjectileDetonationOCL = OCL_Field\nEnd\nWeapon V\n  ProjectileStreamName = None\nEnd\n";
    extra.read(text.as_bytes()).unwrap();
    let sprayed = extra.get("W").unwrap();
    assert_eq!(
        (
            sprayed.secondary_damage,
            sprayed.stream,
            sprayed.detonation_objects
        ),
        (2.5, true, true)
    );
    assert!(!extra.get("V").unwrap().stream);
    assert!(ini.get("rangerrifle").is_none());
    // `SUICIDE` is held by a list of it, or added by `+`, and gone after `NONE`, a list
    // without it, or `-`.
    let affects = |lines: &str| {
        let mut ini = WeaponIni::default();
        ini.read(format!("Weapon W\n{lines}End\n").as_bytes())
            .unwrap();
        ini.get("W").unwrap().affects.suicide
    };
    assert!(affects("  RadiusDamageAffects = ENEMIES SUICIDE\n"));
    assert!(affects("  RadiusDamageAffects = +SUICIDE\n"));
    assert!(!affects(
        "  RadiusDamageAffects = SUICIDE\n  RadiusDamageAffects = NONE\n"
    ));
    assert!(!affects(
        "  RadiusDamageAffects = SUICIDE\n  RadiusDamageAffects = ENEMIES\n"
    ));
    assert!(!affects("  RadiusDamageAffects = +SUICIDE -SUICIDE\n"));
    // `SELF` is the unit itself; a name the game does not list is refused, as its reader
    // refuses it.
    let mut own = WeaponIni::default();
    own.read(b"Weapon W\n  RadiusDamageAffects = SELF NOT_SIMILAR\nEnd\n")
        .unwrap();
    let own = own.get("W").unwrap().affects;
    let expected = RadiusAffects {
        source: true,
        not_similar: true,
        ..RadiusAffects::NONE
    };
    assert_eq!(own, expected);
    for line in [
        "RadiusDamageAffects = FRIENDS",
        "ProjectileCollidesWith = STRUCTURES TREES",
    ] {
        let text = format!("Weapon W\n  {line}\nEnd\n");
        let read = WeaponIni::default().read(text.as_bytes());
        assert_eq!(read, Err(IniError::NotListed { line: 2 }), "{line}");
    }
    // The minigun's stages above 1 and 5 shots give it twice and thrice its rate of fire,
    // names of either case, and its coast is 2,000 ms; its upgrade's damage is no stage's.
    // The spinner's fast stage changes its damage too; a later line of a field takes the
    // earlier's place, so one back to 100 % changes nothing.
    let minigun = ini.get("Minigun").unwrap().continuous_fire;
    assert_eq!(
        (minigun.shots, minigun.coast_ms, minigun.rates()),
        ([1, 5], 2000, [2.0, 3.0])
    );
    assert!(!minigun.changes_other_fields());
    assert!(
        ini.get("Spinner")
            .unwrap()
            .continuous_fire
            .changes_other_fields()
    );
    // The minigun's upgrade gives 125 % of its damage and changes nothing else; a bonus of
    // another field changes one.
    let upgrade = ini.get("Minigun").unwrap().upgrade;
    assert_eq!(
        (upgrade.damage(), upgrade.changes_other_fields()),
        (1.25, false)
    );
    assert_eq!(rifle.upgrade.damage(), 1.0);
    let mut ranged = WeaponIni::default();
    ranged
        .read(b"Weapon W\n  WeaponBonus = PLAYER_UPGRADE RANGE 150%\nEnd\n")
        .unwrap();
    assert!(ranged.get("W").unwrap().upgrade.changes_other_fields());
    let undone = "Weapon W\n  WeaponBonus = CONTINUOUS_FIRE_MEAN DAMAGE 150%\n  \
                  WeaponBonus = CONTINUOUS_FIRE_MEAN DAMAGE 100%\nEnd\n";
    let mut read = WeaponIni::default();
    read.read(undone.as_bytes()).unwrap();
    let undone = read.get("W").unwrap().continuous_fire;
    assert_eq!(undone.rates(), [1.0, 1.0]);
    assert!(!undone.changes_other_fields());
    for line in [
        "WeaponBonus = CONTINUOUS_FIRE_SLOW RATE_OF_FIRE 200%",
        "WeaponBonus = VETERAN SPEED 200%",
    ] {
        let text = format!("Weapon W\n  {line}\nEnd\n");
        let read = WeaponIni::default().read(text.as_bytes());
        assert_eq!(read, Err(IniError::NotListed { line: 2 }), "{line}");
    }
    assert!(!ini.get("Stinger").unwrap().anti_ground);
    // A line outside a block, and a block left open, are refused.
    assert!(WeaponIni::default().read(b"PrimaryDamage = 5\r\n").is_err());
    assert!(WeaponIni::default().read(b"Weapon Open\r\n").is_err());
}
