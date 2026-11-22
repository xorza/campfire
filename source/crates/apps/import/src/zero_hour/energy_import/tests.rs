use campfire_codec::Toml;

use super::*;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// A plant that gives 5, a cannon that takes 3, and a barracks of no power.
const OBJECTS: &str = "Object Plant
  EnergyProduction = 5
  EnergyBonus = 5
End
Object Cannon
  KindOf = POWERED
  EnergyProduction = -3
End
Object Barracks
End
";

/// The shipped low-energy speed: 0.5, 0.8 and a penalty of 1.
const SHIPPED: LowEnergy = LowEnergy {
    min: 0.5,
    max: 0.8,
    penalty: 1.0,
};

/// The power of the fixture's objects at `low`.
fn energy(low: LowEnergy) -> Result<EnergyImport, ZeroHourError> {
    let rules = ZeroHourRules::of_objects(ObjectIni::of(&[OBJECTS]));
    let mut names = ImportNames::default();
    for object in ["Plant", "Cannon", "Barracks"] {
        let object = rules.objects.object(object.as_bytes()).unwrap();
        names.unit_type(object, &rules).unwrap();
    }
    let names = names.into_production().into_projectiles();
    EnergyImport::of(low, &rules.names, names.types())
}

#[test]
fn the_power_is_the_game_datas_speed_of_trains_and_builds_and_its_plants_give_power() {
    let import = energy(SHIPPED).unwrap();
    // The `f32`s' nearest billionths: 0.5 and 1 exactly, 0.8 as 0.800000012.
    let speed = import.power.speed.unwrap();
    let parts = [speed.penalty, speed.min, speed.max].map(Share::parts);
    assert_eq!(parts, [1_000_000_000, 500_000_000, 800_000_012]);
    assert_eq!(speed.empty, EmptyPower::None);
    assert_eq!(speed.slows, [ActionKind::Train, ActionKind::Build]);
    assert_eq!(
        (import.power.stat, import.power.short),
        (None, Some(DeclaredName::new("underpowered").unwrap()))
    );
    // The shortage's modifier gives the player's powered units the tag `disabled`.
    let shortage = Toml::write(&import.shortage.unwrap());
    assert_eq!(
        shortage,
        "affects = \"allies:powered\"\ntags = [\"disabled\"]\n"
    );
    // The plant alone gives power; the cannon takes it.
    let plants: Vec<&str> = import.plants.iter().map(DeclaredName::as_str).collect();
    assert_eq!(plants, ["plant"]);
    // A bound of none is the original's floor of 0.01, and a `min` past its `max` the `max`: the
    // game's defaults, none of each, slow a short player to 0.01; a `min` of none to 0.01 at most
    // 0.8; a `min` of 0.9 is the `max` of 0.8.
    let bounds = |low: LowEnergy| {
        let speed = energy(low).unwrap().power.speed.unwrap();
        [speed.min, speed.max].map(Share::parts)
    };
    let none = LowEnergy {
        min: 0.0,
        max: 0.0,
        penalty: 0.0,
    };
    assert_eq!(bounds(none), [10_000_000; 2]);
    assert_eq!(
        bounds(LowEnergy {
            min: 0.0,
            ..SHIPPED
        }),
        [10_000_000, 800_000_012]
    );
    assert_eq!(
        bounds(LowEnergy {
            min: 0.9,
            ..SHIPPED
        }),
        [800_000_012; 2]
    );
    // A penalty past 1 or below 0 is none the engine holds.
    for penalty in [1.5, -0.5] {
        let low = LowEnergy { penalty, ..SHIPPED };
        assert!(
            matches!(energy(low), Err(ZeroHourError::LowEnergy)),
            "{low:?}"
        );
    }
}
