use super::*;
use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;
use crate::zero_hour::format::ini::object_ini::ObjectIni;
use crate::zero_hour::format::ini::object_ini::samples::{DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS};
use crate::zero_hour::format::ini::player_template_ini::samples::PLAYER_TEMPLATES;

#[test]
fn a_skirmish_starts_each_playable_faction_with_its_objects_and_offset() {
    let start = |templates: &str| {
        let mut rules = ZeroHourRules {
            objects: ObjectIni::of(&[DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS]),
            ..ZeroHourRules::default()
        };
        rules.locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
        rules.templates.read(templates.as_bytes()).unwrap();
        SkirmishStart::of(&rules, &mut ImportNames::default())
    };
    // America's command center, of a sphere of radius 25, and its dozer, 12.5 m along +z;
    // the observer, which no player plays, is no faction.
    let name = |name: &str| DeclaredName::new(name).unwrap();
    let america = FactionStart {
        name: name("america"),
        building: name("americacommandcenter"),
        units: vec![name("americavehicledozer")],
        offset: Num::int(25) / Num::int(2),
    };
    assert_eq!(
        start(PLAYER_TEMPLATES).unwrap(),
        SkirmishStart {
            factions: vec![america],
        }
    );
    // The factions in the order the files first define them, each named by its template's
    // name less its lead `Faction`, or whole when it has none.
    let more = format!(
        "{PLAYER_TEMPLATES}PlayerTemplate Rebels\n PlayableSide = Yes\n \
             StartingBuilding = AmericaCommandCenter\nEnd\n"
    );
    let factions = start(&more).unwrap().factions;
    let names: Vec<&str> = factions
        .iter()
        .map(|faction| faction.name.as_str())
        .collect();
    assert_eq!(names, ["america", "rebels"]);
    assert!(factions[1].units.is_empty());
    // No template a player plays, or one of no building, is refused.
    let refused = start("PlayerTemplate FactionChina\n StartingBuilding = Lost\nEnd\n");
    assert!(
        matches!(refused, Err(ZeroHourError::NoFaction)),
        "{refused:?}"
    );
    let refused = start("PlayerTemplate FactionAmerica\n PlayableSide = Yes\nEnd\n");
    assert!(
        matches!(&refused, Err(ZeroHourError::NoStart(name)) if name == "FactionAmerica"),
        "{refused:?}"
    );
    for templates in [
        "PlayerTemplate FactionAmerica\n PlayableSide = Yes\n StartingBuilding = Lost\nEnd\n",
        "PlayerTemplate FactionAmerica\n PlayableSide = Yes\n \
             StartingBuilding = AmericaCommandCenter\n StartingUnit0 = Lost\nEnd\n",
    ] {
        let refused = start(templates);
        assert!(
            matches!(&refused, Err(ZeroHourError::NoStartObject(name)) if name == "Lost"),
            "{refused:?}"
        );
    }
}
