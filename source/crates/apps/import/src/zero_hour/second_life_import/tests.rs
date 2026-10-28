use campfire_combat::TaggedArmorData;

use super::*;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::imported_type::ImportedType;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// A Fire Base of a hive body, and a Battle Bus of an undead body, as the release game gives them,
/// of an armor set for each life; and an undead body that names no second life's most.
const OBJECTS: &str = "Object FireBase
  KindOf = STRUCTURE
  Body = HiveStructureBody ModuleTag_04
    MaxHealth = 1000.0
    InitialHealth = 1000.0
  End
  Geometry = BOX
  GeometryMajorRadius = 26.0
  GeometryMinorRadius = 26.0
End
Object BattleBus
  KindOf = VEHICLE
  ArmorSet
    Conditions = None
    Armor = BattleBusTruckArmor
  End
  ArmorSet
    Conditions = SECOND_LIFE
    Armor = BattleBusStructureArmorTough
  End
  ArmorSet
    Conditions = CRATE_UPGRADE_ONE SECOND_LIFE
    Armor = BattleBusStructureArmorToughPlusOne
  End
  Body = UndeadBody ModuleTag_02
    MaxHealth = 400.0
    InitialHealth = 400.0
    SecondLifeMaxHealth = 650.0
  End
End
Object PlainUndead
  KindOf = VEHICLE
  Body = UndeadBody ModuleTag_02
    MaxHealth = 400.0
  End
End
";

#[test]
fn a_hive_lives_as_a_structure_and_an_undead_body_lives_twice_by_a_shared_modifier() {
    let rules = ZeroHourRules {
        objects: ObjectIni::of(&[OBJECTS]),
        ..ZeroHourRules::default()
    };
    let of = |name: &str| ImportedType::of(rules.objects.object(name.as_bytes()).unwrap(), &rules);
    let [fire_base, bus] = ["FireBase", "BattleBus"].map(|name| of(name).unwrap());
    // The hive's 1,000 is its life, and it lives once.
    assert_eq!(
        (fire_base.life(), fire_base.second_life()),
        (Some(Num::int(1000)), None)
    );
    // The bus lives its 400, then 650, 250 more, its second life's armor first, tested by the tag
    // of a unit in its second life; the set of a crate's upgrade too is not converted.
    assert_eq!(
        (bus.life(), bus.second_life()),
        (Some(Num::int(400)), Some(Num::int(250)))
    );
    // An undead body of no `SecondLifeMaxHealth` lives its second life at the module's 1, 399
    // below its first 400.
    assert_eq!(
        of("PlainUndead").unwrap().second_life(),
        Some(Num::int(-399))
    );
    let armors = bus.combat().unwrap().armors.clone().unwrap();
    assert_eq!(
        armors,
        [TaggedArmorData {
            when: rules.names.in_second_life.clone(),
            armor: DeclaredName::new("battlebusstructurearmortough").unwrap(),
        }]
    );
    assert_eq!(
        rules
            .objects
            .object(b"BattleBus")
            .unwrap()
            .fight()
            .unconverted_sets(),
        1
    );
    // One modifier for each raise, adding it to the life stat and giving the tag; the bus's
    // second life names it, and dies through it by `UNRESISTABLE` damage alone.
    let lives = SecondLifeImport::of([Num::int(250), Num::int(250)], &rules.names);
    let name = DeclaredName::new("second_life_1").unwrap();
    assert_eq!(
        lives.modifiers,
        BTreeMap::from([(
            name.clone(),
            SecondLifeModifier {
                stats: BTreeMap::from([(
                    rules.names.life_stat.clone(),
                    Number::from(Num::int(250))
                )]),
                tags: vec![rules.names.second_life.clone()],
            }
        )])
    );
    assert_eq!(
        lives.data(Num::int(250)),
        SecondLifeData {
            modifier: name,
            lethal: vec![DeclaredName::new("unresistable").unwrap()],
        }
    );
}
