use campfire_codec::Toml;

use super::*;
use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::import_names::ImportNames;

/// Riders of each kind a container tells apart, and a container of each module and each loss.
const CONTAINERS: &str = "Object Ranger
  KindOf = INFANTRY
  Locomotor = SET_NORMAL TankTreads
  TransportSlotCount = 1
End
Object Guard
  KindOf = INFANTRY
  Locomotor = SET_NORMAL TankTreads
  TransportSlotCount = 1
End
Object Burton
  KindOf = INFANTRY NO_GARRISON
  Locomotor = SET_NORMAL TankTreads
  TransportSlotCount = 1
End
Object Crusader
  KindOf = VEHICLE
  Locomotor = SET_NORMAL TankTreads
  TransportSlotCount = 3
End
Object Overlord
  KindOf = VEHICLE HUGE_VEHICLE
  Locomotor = SET_NORMAL TankTreads
  TransportSlotCount = 3
End
Object Dozer
  KindOf = DOZER
  Locomotor = SET_NORMAL TankTreads
  TransportSlotCount = 1
End
Object Bomb
  KindOf = PROJECTILE
  TransportSlotCount = 10
End
Object Humvee
  KindOf = VEHICLE
  Locomotor = SET_NORMAL TankTreads
  TransportSlotCount = 3
  Behavior = TransportContain ModuleTag_05
    PassengersAllowedToFire = Yes
    Slots = 5
    DamagePercentToUnits = 100%
    AllowInsideKindOf = INFANTRY
    ExitDelay = 250
    NumberOfExitPaths = 3
  End
End
Object Crawler
  KindOf = VEHICLE
  Locomotor = SET_NORMAL TankTreads
  Behavior = TransportContain ModuleTag_05
    Slots = 8
    InitialPayload = Guard 8
    HealthRegen%PerSec = 10
    DamagePercentToUnits = 10%
    ExitDelay = 250
  End
  Behavior = AssaultTransportAIUpdate ModuleTag_NewAI
    MembersGetHealedAtLifeRatio = 0.5
  End
End
Object Overfull
  KindOf = VEHICLE
  Locomotor = SET_NORMAL TankTreads
  Behavior = TransportContain ModuleTag_05
    Slots = 8
    InitialPayload = Guard 9
  End
End
Object Chinook
  KindOf = VEHICLE AIRCRAFT
  Locomotor = SET_NORMAL TankTreads
  Behavior = TransportContain ModuleTag_06
    Slots = 8
    DamagePercentToUnits = 100%
    AllowInsideKindOf = INFANTRY VEHICLE
    ForbidInsideKindOf = AIRCRAFT HUGE_VEHICLE
    ExitDelay = 100
  End
End
Object Chapel
  KindOf = STRUCTURE
  Behavior = GarrisonContain ModuleTag_03
    ContainMax = 10
  End
End
Object Bunker
  KindOf = STRUCTURE FS_BASE_DEFENSE GARRISONABLE_UNTIL_DESTROYED
  Behavior = GarrisonContain ModuleTag_03
    ContainMax = 5
    AllowAlliesInside = Yes
    AllowEnemiesInside = No
    AllowNeutralInside = No
  End
End
Object Firebase
  KindOf = STRUCTURE FS_BASE_DEFENSE
  Behavior = GarrisonContain ModuleTag_03
    ContainMax = 4
    IsEnclosingContainer = No
  End
End
Object Hospital
  KindOf = STRUCTURE
  Behavior = HealContain ModuleTag_05
  End
End
Object Bomber
  Behavior = TransportContain ModuleTag_06
    Slots = 10
    AllowInsideKindOf = PROJECTILE
  End
End
Object Loader
  Behavior = TransportContain ModuleTag_06
    Slots = 2
    AllowInsideKindOf = INFANTRY DOZER
  End
End
Object Locked
  Behavior = TransportContain ModuleTag_06
    Slots = 2
    AllowAlliesInside = No
  End
End
Object Empty
  Behavior = TransportContain ModuleTag_06
  End
End
Object Odd
  Behavior = TransportContain ModuleTag_06
    Slots = 2
    AllowInsideKindOf = INFANTRY SALVAGER
  End
End
Object Fortress
  KindOf = VEHICLE
  Locomotor = SET_NORMAL TankTreads
  Behavior = OverlordContain ModuleTag_06
    Slots = 1
    PassengersAllowedToFire = Yes
  End
  Behavior = ObjectCreationUpgrade ModuleTag_09
    UpgradeObject = OCL_FortressBunker
    TriggeredBy = Upgrade_FortressBunker
  End
End
Object FortressBunker
  KindOf = PORTABLE_STRUCTURE
  Behavior = TransportContain ModuleTag_03
    Slots = 5
    PassengersAllowedToFire = Yes
    AllowInsideKindOf = INFANTRY
  End
End
Object Tower
  KindOf = VEHICLE
  Locomotor = SET_NORMAL TankTreads
  Behavior = OverlordContain ModuleTag_06
    Slots = 1
  End
End
Object Helix
  KindOf = VEHICLE AIRCRAFT
  Locomotor = SET_NORMAL TankTreads
  Behavior = HelixContain ModuleTag_29
    Slots = 5
    DamagePercentToUnits = 100%
    AllowInsideKindOf = INFANTRY VEHICLE PORTABLE_STRUCTURE
    ForbidInsideKindOf = AIRCRAFT
    ExitDelay = 100
    PassengersAllowedToFire = No
  End
End
";

/// The list a fortress's upgrade runs: its bunker, made inside it.
const LISTS: &str = "ObjectCreationList OCL_FortressBunker
  CreateObject
    ObjectNames = FortressBunker
    ContainInsideSourceObject = Yes
  End
End
";

#[test]
fn transports_and_garrisons_convert_to_their_entries_and_riders_to_their_sizes() {
    let mut rules = ZeroHourRules {
        objects: ObjectIni::of(&[CONTAINERS]),
        ..ZeroHourRules::default()
    };
    rules.locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    rules.creation_lists.read(LISTS.as_bytes()).unwrap();
    rules
        .game_data
        .read(
            b"GameData\n UnitReallyDamagedThreshold = 0.35\n WeaponBonus = GARRISONED RANGE 133%\n \
              WeaponBonus = GARRISONED DAMAGE 125%\nEnd\n",
        )
        .unwrap();
    let mut names = ImportNames::default();
    for object in rules.objects.objects() {
        names.unit_type(object, &rules).unwrap();
    }
    let names = names.into_production().into_projectiles();
    let type_name = |name: &str| TypeName::named(name);
    // The chapel's model gives two fire points, the game's `(10, 5)` and `(−10, 5)`.
    let fire_points = BTreeMap::from([(
        type_name("chapel").declared().clone(),
        vec![[10.0, 5.0], [-10.0, 5.0]],
    )]);
    let import = ContainImport::of(&rules, names.types(), &fire_points).unwrap();
    // Each type that walks and takes room of a transport rides, of that room: not the bomb,
    // which does not walk, nor the Chinook, of none.
    let size = |size| RidesData {
        size: NonZeroU32::new(size).unwrap(),
    };
    let rides = [
        ("burton", 1),
        ("crusader", 3),
        ("dozer", 1),
        ("guard", 1),
        ("humvee", 3),
        ("overlord", 3),
        ("ranger", 1),
    ]
    .map(|(name, room)| (type_name(name), size(room)));
    assert_eq!(import.rides, BTreeMap::from(rides));
    let filters = |filters: &[&str]| -> Vec<FilterData> {
        filters
            .iter()
            .map(|filter| FilterData::parse(filter).unwrap())
            .collect()
    };
    let share = |billionths| Share::billionths(billionths).unwrap();
    let enter = |name: &str| import.enters[&type_name(name)].clone();
    // The Humvee takes 5 of its own player's infantry by their room, lets one out each 250 ms,
    // ⌈250 × 0.03⌉ = 8 frames, written back as ⌊8 × 100 / 3⌋ = 266 ms, kills each as it dies,
    // and lets its infantry fire from its place.
    let humvee = EnterData {
        action: DeclaredName::new("enter").unwrap(),
        capacity: 5,
        counts: RiderCount::Size,
        riders: filters(&["all:infantry"]),
        exit_ms: 266,
        damage: Share::ALL,
        full: Share::NONE,
        capture: false,
        payload: Vec::new(),
        fire: FireData::Center(TagFilterData::new([FilterTag {
            name: rules.names.infantry.clone(),
            negated: false,
        }])),
        sight: false,
        modifier: None,
        open: None,
        assault: None,
    };
    assert_eq!(enter("humvee"), humvee);
    // The crawler comes with its 8 Guards, deals each 10 % of its life, the game's `f32` of
    // 0.100000001490116… to the nearest billionth, lets none fire, and lets out to fight those
    // above half their life as it is ordered to attack; its heal is lost. The one of 9 Guards,
    // past its room and of no assault, comes empty.
    let crawler = EnterData {
        capacity: 8,
        damage: share(100_000_001),
        payload: vec![PayloadData {
            unit: type_name("guard").declared().clone(),
            count: 8,
        }],
        fire: FireData::None,
        assault: Some(share(500_000_000)),
        ..humvee.clone()
    };
    assert_eq!(enter("crawler"), crawler);
    assert_eq!(
        enter("overfull"),
        EnterData {
            damage: Share::NONE,
            exit_ms: 0,
            payload: Vec::new(),
            assault: None,
            ..crawler
        }
    );
    // The Chinook takes infantry or a vehicle, but an aircraft or a huge vehicle, each 100 ms,
    // ⌈100 × 0.03⌉ = 3 frames, apart.
    assert_eq!(
        enter("chinook"),
        EnterData {
            capacity: 8,
            riders: filters(&[
                "all:infantry:!aircraft:!huge_vehicle",
                "all:vehicle:!aircraft:!huge_vehicle"
            ]),
            exit_ms: 100,
            fire: FireData::None,
            ..humvee.clone()
        }
    );
    // The fortress takes its bunker's 5 infantry, which fire, once it holds its bunker's upgrade's
    // tag, the upgrade's name suffixed `created`; the Helix 5 infantry or vehicles but an aircraft,
    // each 100 ms apart, which do not fire (04-capabilities/interaction.md, A bunker is an entry a
    // tag opens). The tower of no bunker's upgrade goes.
    let opened = FilterTag {
        name: DeclaredName::new("upgrade_fortressbunker_created").unwrap(),
        negated: false,
    };
    assert_eq!(
        enter("fortress"),
        EnterData {
            damage: Share::NONE,
            exit_ms: 0,
            open: Some(TagFilterData::new([opened])),
            ..humvee.clone()
        }
    );
    assert_eq!(
        enter("helix"),
        EnterData {
            riders: filters(&["all:infantry:!aircraft", "all:vehicle:!aircraft"]),
            exit_ms: 100,
            fire: FireData::None,
            ..humvee.clone()
        }
    );
    // The chapel takes 10 infantry of anyone but those of `NO_GARRISON`, one each, all at once,
    // and none at the game's 0.35 of its life, 0.349999994… to the nearest billionth, or below;
    // it takes its first rider's side, holds the garrison's modifier on its riders, lets them see
    // from it, and lets them fire from its points, `(x, −y)` on the engine's ground.
    let point = |x: i64, z: i64| [Num::int(x), Num::int(z)];
    let chapel = EnterData {
        action: humvee.action,
        capacity: 10,
        counts: RiderCount::One,
        riders: filters(&["all:infantry:!no_garrison"]),
        exit_ms: 0,
        damage: Share::NONE,
        full: share(349_999_994),
        capture: true,
        payload: Vec::new(),
        fire: FireData::Points(vec![point(10, -5), point(-10, -5)]),
        sight: true,
        modifier: Some(DeclaredName::new("garrisoned").unwrap()),
        open: None,
        assault: None,
    };
    assert_eq!(enter("chapel"), chapel);
    // The bunker, a faction's, takes its allies alone, lasts until it dies, and lets its riders
    // fire from its place, of no fire point; the fire base's riders fire from it too, of its
    // station points.
    let bunker = EnterData {
        capacity: 5,
        riders: filters(&["allies:infantry:!no_garrison"]),
        full: Share::NONE,
        capture: false,
        fire: FireData::Center(TagFilterData::new([])),
        ..chapel
    };
    assert_eq!(enter("bunker"), bunker);
    assert_eq!(
        enter("firebase"),
        EnterData {
            capacity: 4,
            riders: filters(&["all:infantry:!no_garrison"]),
            full: share(349_999_994),
            ..bunker
        }
    );
    // Each other module, and each container no tag of the mode can say who rides, goes: the
    // hospital's; the bomber, which takes no unit that rides; the loader, which takes a dozer of
    // no tag; the locked, which takes no ally; the empty, of no room; the odd, of a kind the
    // import does not read. The crawler's heal, the fire base's station points and the
    // overfull's payload are lost.
    let dropped: Vec<(&str, ContainDropCause)> = (&import.dropped)
        .into_iter()
        .map(|dropped| (dropped.name.as_str(), dropped.cause))
        .collect();
    assert_eq!(
        dropped,
        [
            ("Bomber", ContainDropCause::NoRider),
            ("Crawler", ContainDropCause::RiderHeal),
            ("Empty", ContainDropCause::NoRoom),
            ("Firebase", ContainDropCause::StationPoints),
            ("Hospital", ContainDropCause::Heal),
            ("Loader", ContainDropCause::Kinds),
            ("Locked", ContainDropCause::Relations),
            ("Odd", ContainDropCause::Kinds),
            ("Overfull", ContainDropCause::Payload),
            ("Tower", ContainDropCause::Overlord),
        ]
    );
    let converted: Vec<&str> = import.enters.keys().map(TypeName::as_str).collect();
    assert_eq!(
        converted,
        [
            "bunker",
            "chapel",
            "chinook",
            "crawler",
            "firebase",
            "fortress",
            "fortressbunker",
            "helix",
            "humvee",
            "overfull"
        ]
    );
    // The garrison's modifier adds the game's 1.33 less 1 to the range and 0.25 to the damage.
    let range = game_float::to_num(1.33_f32 - 1.0).unwrap();
    assert_eq!(
        Toml::write(&import.modifiers[&rules.names.garrisoned]),
        format!("[stats]\ngarrison_damage = \"0.25\"\ngarrison_range = \"{range}\"\n")
    );
}

#[test]
fn a_share_is_the_game_s_float_to_the_nearest_billionth() {
    // 1 and 0 are whole; 0.1 is 0.100000001490116…, 100,000,001 billionths; past 1, below 0
    // or not a number is none.
    assert_eq!(ContainImport::share(1.0), Some(Share::ALL));
    assert_eq!(ContainImport::share(0.0), Some(Share::NONE));
    assert_eq!(ContainImport::share(0.1), Share::billionths(100_000_001));
    for refused in [1.000_001, -0.1, f32::NAN] {
        assert_eq!(ContainImport::share(refused), None, "{refused}");
    }
}
