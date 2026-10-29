use campfire_math::Num;

use super::*;
use crate::zero_hour::format::ini::object_ini::ObjectFiles;

/// A tank, a car and a soldier, each of its own normal locomotor, a tank of no look, and a tank
/// whose pitch stiffness no `Num` holds.
const OBJECTS: &str = "Object Tank
  Locomotor = SET_NORMAL Treads
End
Object Car
  Locomotor = SET_NORMAL Wheels
End
Object Soldier
  Locomotor = SET_NORMAL Legs
End
Object Unseen
  Locomotor = SET_NORMAL Treads
End
Object Broken
  Locomotor = SET_NORMAL Stiff
End
";

/// Treads of springs of their own, wheels of the defaults, and legs.
const LOCOMOTORS: &str = "Locomotor Treads
  Surfaces = GROUND
  Appearance = TREADS
  PitchStiffness = 0.2
  RollStiffness = 0.3
  PitchDamping = 0.8
  RollDamping = 0.7
  UniformAxialDamping = 0.5
  ForwardAccelerationPitchFactor = 2
  LateralAccelerationRollFactor = 4
  AccelerationPitchLimit = 3
  DecelerationPitchLimit = 6
End
Locomotor Wheels
  Surfaces = GROUND
  Appearance = FOUR_WHEELS
End
Locomotor Legs
  Surfaces = GROUND
  Appearance = TWO_LEGS
End
Locomotor Stiff
  Surfaces = GROUND
  Appearance = TREADS
  PitchStiffness = 1e40
End
";

#[test]
fn a_tracked_or_wheeled_look_takes_its_locomotors_chassis() {
    let mut files = ObjectFiles::default();
    files.read(OBJECTS.as_bytes()).unwrap();
    let objects = files.finish();
    let mut locomotors = LocomotorIni::default();
    locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
    let name = |text: &str| DeclaredName::new(text).unwrap();
    let mut looks: BTreeMap<DeclaredName, ClientUnit> = ["tank", "car", "soldier", "broken"]
        .map(|look| (name(look), ClientUnit::new(Vec::new())))
        .into();
    let losses = ChassisImport::name(&objects, &locomotors, &mut looks).unwrap();
    let num = |value: f32| game_float::to_num(value).unwrap();
    // The tank's springs, each its own, its limits of 3° and 6° in radians.
    assert_eq!(
        looks[&name("tank")].chassis,
        Some(Chassis {
            pitch_stiffness: num(0.2),
            roll_stiffness: num(0.3),
            pitch_damping: num(0.8),
            roll_damping: num(0.7),
            axial_damping: num(0.5),
            forward_pitch: num(2.0),
            lateral_roll: num(4.0),
            accelerating: num(3.0_f32.to_radians()),
            braking: num(6.0_f32.to_radians()),
        })
    );
    // The car's, the template's defaults: stiffness 0.1, damping 0.9, its axes kept whole, and
    // no kick or limit.
    assert_eq!(
        looks[&name("car")].chassis,
        Some(Chassis {
            pitch_stiffness: num(0.1),
            roll_stiffness: num(0.1),
            pitch_damping: num(0.9),
            roll_damping: num(0.9),
            axial_damping: Num::ONE,
            forward_pitch: Num::ZERO,
            lateral_roll: Num::ZERO,
            accelerating: Num::ZERO,
            braking: Num::ZERO,
        })
    );
    // Legs carry no chassis, and an object of no look gains none.
    assert_eq!(looks[&name("soldier")].chassis, None);
    assert!(!looks.contains_key(&name("unseen")));
    // A stiffness of 10⁴⁰ drops the broken tank's chassis alone, counted.
    assert_eq!(looks[&name("broken")].chassis, None);
    assert_eq!(
        losses,
        [Dropped {
            name: "Broken".to_owned(),
            cause: EffectDropCause::Number,
        }]
    );
}
