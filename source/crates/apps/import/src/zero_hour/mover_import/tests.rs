use campfire_stats::StatValue;

use super::*;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// The release game's locomotors of a Crusader, a Humvee and a Ranger, as the research reads them.
const LOCOMOTORS: &str = "Locomotor CrusaderLocomotor
  Surfaces = GROUND RUBBLE
  Speed = 30
  SpeedDamaged = 25
  TurnRate = 180
  Acceleration = 1000
  Braking = 1000
  MinTurnSpeed = 0
  Appearance = TREADS
End
Locomotor HumveeLocomotor
  Surfaces = GROUND RUBBLE
  Speed = 60
  SpeedDamaged = 30
  TurnRate = 180
  Acceleration = 1000
  Braking = 1000
  MinTurnSpeed = 20
  TurnPivotOffset = -0.33
  CanMoveBackwards = Yes
  Appearance = FOUR_WHEELS
End
Locomotor BasicHumanLocomotor
  Surfaces = GROUND RUBBLE
  Speed = 20
  SpeedDamaged = 10
  TurnRate = 500
  Acceleration = 100
  Braking = 100
  Appearance = TWO_LEGS
End
";

/// Objects of those locomotors: the Humvee a box of half extents 9 and 12, of a bounding circle of
/// 15, losing 3 of its lateral velocity a second.
const OBJECTS: &str = "Object Crusader
  Locomotor = SET_NORMAL CrusaderLocomotor
  Geometry = BOX
  GeometryMajorRadius = 12.0
  GeometryMinorRadius = 8.0
End
Object Humvee
  Locomotor = SET_NORMAL HumveeLocomotor
  Geometry = BOX
  GeometryMajorRadius = 9.0
  GeometryMinorRadius = 12.0
  Behavior = PhysicsBehavior ModuleTag_01
    LateralFriction = 3.0
  End
End
Object Ranger
  Locomotor = SET_NORMAL BasicHumanLocomotor
  Geometry = CYLINDER
  GeometryMajorRadius = 10.0
End
";

#[test]
fn a_mover_takes_its_locomotors_numbers_and_its_hurt_shares_a_modifier_by_its_change() {
    let mut rules = ZeroHourRules {
        ..ZeroHourRules::of_objects(ObjectIni::of(&[OBJECTS]))
    };
    rules.locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    rules.game_data.really_damaged_threshold = 0.35;
    let of = |name: &str| ImportedType::of(rules.objects.object(name.as_bytes()).unwrap(), &rules);
    let [crusader, humvee, ranger] = ["Crusader", "Humvee", "Ranger"].map(|name| of(name).unwrap());
    let mover = |imported: &ImportedType| *imported.mover().unwrap();
    let below: Share = "0.35".parse().unwrap();
    let hurt = |speed: i64| {
        Some(MoverHurt {
            below,
            change: HurtChange {
                speed: Num::int(speed),
                ..HurtChange::NONE
            },
        })
    };
    // The Crusader's treads: braking 1,000, turning at no least speed, of no pivot, the default
    // friction, 0.15 to the bit; five major radii, 60 m, for a three-point turn no tread makes;
    // hurt by 25 − 30 = −5 at 0.35 of its life.
    assert_eq!(
        mover(&crusader),
        ImportedMover {
            locomotor: ZeroHourLocomotor {
                three_point: Num::int(60),
                ..ZeroHourLocomotor::of(LocomotorAppearance::Treads, Num::int(1000))
            },
            acceleration: Num::int(1000),
            hurt: hurt(-5),
        }
    );
    // The Humvee's wheels: turning at 20 at least, backing, three points past 5 × 9 = 45 m; its
    // pivot −0.33 of its radius of 15, in `f32` −0.33000001 × 15 = −4.9500002861…, exactly
    // −83,047,224 · 2⁻²⁴; its friction 3 a second times the `f32` of 1 / 30, 0.1000000089…, to
    // the nearest bit 1,677,722 · 2⁻²⁴; hurt by 30 − 60 = −30.
    assert_eq!(
        mover(&humvee),
        ImportedMover {
            locomotor: ZeroHourLocomotor {
                min_turn_speed: Num::int(20),
                pivot: Num::from_bits(-83_047_224),
                backwards: true,
                three_point: Num::int(45),
                lateral_friction: Num::from_bits(1_677_722),
                ..ZeroHourLocomotor::of(LocomotorAppearance::FourWheels, Num::int(1000))
            },
            acceleration: Num::int(1000),
            hurt: hurt(-30),
        }
    );
    // The Ranger's legs: braking 100; its turning speed the template's `BIGNUM`, 99,999 a frame,
    // past the world's bound, so the bound; hurt by 10 − 20 = −10.
    assert_eq!(
        mover(&ranger),
        ImportedMover {
            locomotor: ZeroHourLocomotor {
                min_turn_speed: Vec3::BOUND,
                three_point: Num::int(50),
                ..ZeroHourLocomotor::of(LocomotorAppearance::TwoLegs, Num::int(100))
            },
            acceleration: Num::int(100),
            hurt: hurt(-10),
        }
    );
    // A mover's acceleration is a stat of its type, beside its speed and turn rate.
    assert_eq!(
        crusader
            .stats()
            .value(&Stat::Engine(EngineStat::Acceleration)),
        Some(StatValue::flat(Num::int(1000)))
    );
    // One modifier for each change, in the changes' order: the Humvee's −30 first, the Ranger's
    // −10, then the Crusader's −5; each adds its change to `move_speed` alone, and each type's
    // `hurt` names its own.
    let movers = MoverImport::of([&crusader, &humvee, &ranger].map(mover).iter());
    let adds = |speed: i64| HurtModifier {
        stats: BTreeMap::from([(
            Stat::Engine(EngineStat::MoveSpeed),
            Number::from(Num::int(speed)),
        )]),
    };
    let name = |at: u8| DeclaredName::new(&format!("really_damaged_{at}")).unwrap();
    assert_eq!(
        movers.modifiers,
        BTreeMap::from([
            (name(1), adds(-30)),
            (name(2), adds(-10)),
            (name(3), adds(-5))
        ])
    );
    let named = |imported: &ImportedType| movers.hurt(imported.mover().unwrap()).unwrap();
    assert_eq!(
        named(&humvee),
        HurtData {
            below,
            modifier: name(1),
        }
    );
    assert_eq!(named(&crusader).modifier, name(3));
}

#[test]
fn a_locomotor_of_no_movers_appearance_or_no_damaged_change_gives_no_mover_or_no_hurt() {
    let locomotors = "Locomotor Even
  Surfaces = GROUND
  Speed = 20
  TurnRate = 90
  Appearance = TREADS
End
Locomotor Climbing
  Surfaces = GROUND CLIFF
  Speed = 20
  Appearance = CLIMBER
End
";
    let objects = "Object Even
  Locomotor = SET_NORMAL Even
  Geometry = CYLINDER
  GeometryMajorRadius = 1.0
End
Object Climber
  Locomotor = SET_NORMAL Climbing
  Geometry = CYLINDER
  GeometryMajorRadius = 1.0
End
";
    let mut rules = ZeroHourRules {
        ..ZeroHourRules::of_objects(ObjectIni::of(&[objects]))
    };
    rules.locomotors.read(locomotors.as_bytes()).unwrap();
    let of = |name: &str| ImportedType::of(rules.objects.object(name.as_bytes()).unwrap(), &rules);
    // Its damaged numbers its own: a mover of no hurt, and none in the modifiers.
    let even = of("Even").unwrap();
    assert_eq!(even.mover().unwrap().hurt, None);
    assert!(MoverImport::of(even.mover()).modifiers.is_empty());
    // A climber steps as the engine's walkers do.
    let climber = of("Climber").unwrap();
    assert_eq!(climber.mover(), None);
    assert!(
        !climber
            .stats()
            .declares(&Stat::Engine(EngineStat::Acceleration))
    );
}
