use campfire_common::DeclaredName;
use campfire_math::Num;

use crate::zero_hour::error::{TypeError, ZeroHourError};
use crate::zero_hour::format::game_float;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::player_template_ini::PlayerTemplate;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// The lead of a template's name that its faction's name drops: `FactionChina` is `china`.
const LEAD: &str = "Faction";

/// What a skirmish starts each player with, by the faction it picked, which the mode's params
/// give its script (09-zero-hour.md, A skirmish starts with the faction's start objects;
/// 09-zero-hour.md, Each player picks a faction): each template a player plays, in the
/// store's order, the order its lobby lists them in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkirmishStart {
    pub(crate) factions: Vec<FactionStart>,
}

/// A faction's start: its name, its template's without the lead `Faction`, its building and
/// starting units, by their unit types, and how far along `+z` from the building its units start
/// on a map with no rally, half the building's bounding sphere, as
/// `placeNetworkBuildingsForPlayer` moves them toward the original's `−y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FactionStart {
    pub(crate) name: DeclaredName,
    pub(crate) building: DeclaredName,
    pub(crate) units: Vec<DeclaredName>,
    pub(crate) offset: Num,
}

impl SkirmishStart {
    /// The start of each template of `rules` a player plays, its objects unit types `names`
    /// takes; an error for no such template, one of no building, an object the install lacks,
    /// or an object that gives no unit type.
    pub(crate) fn of(
        rules: &ZeroHourRules,
        names: &mut ImportNames,
    ) -> Result<Self, ZeroHourError> {
        let factions = rules
            .templates
            .playable()
            .map(|(template_name, template)| {
                FactionStart::of(rules, names, template_name, template)
            })
            .collect::<Result<Vec<_>, _>>()?;
        if factions.is_empty() {
            return Err(ZeroHourError::NoFaction);
        }
        Ok(Self { factions })
    }
}

impl FactionStart {
    /// The start of `template`, of the name `template_name`, as `SkirmishStart::of` makes each.
    fn of(
        rules: &ZeroHourRules,
        names: &mut ImportNames,
        template_name: &str,
        template: &PlayerTemplate,
    ) -> Result<Self, ZeroHourError> {
        let (ini, locomotors) = (&rules.objects, &rules.locomotors);
        let no_start = || ZeroHourError::NoStart(template_name.to_owned());
        let building = template.building.as_deref().ok_or_else(no_start)?;
        let object = |name: &str| {
            ini.object(name.as_bytes())
                .ok_or_else(|| ZeroHourError::NoStartObject(name.to_owned()))
        };
        let mut unit_type = |object: &IniObject| {
            names
                .unit_type(object, locomotors)
                .map(|named| named.name)
                .map_err(|error| Self::type_error(object, error))
        };
        let building = object(building)?;
        let half = building.geometry().bounding_sphere() / 2.0;
        let offset =
            game_float::to_num(half).map_err(|_| Self::type_error(building, TypeError::Number))?;
        let faction = template_name.strip_prefix(LEAD).unwrap_or(template_name);
        Ok(Self {
            name: ImportName::of(faction.as_bytes()).declared(),
            building: unit_type(building)?,
            units: template
                .units()
                .map(|name| unit_type(object(name)?))
                .collect::<Result<_, _>>()?,
            offset,
        })
    }

    fn type_error(object: &IniObject, error: TypeError) -> ZeroHourError {
        ZeroHourError::StartType {
            object: object.name.clone(),
            error,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zero_hour::format::ini::locomotor_ini::samples::LOCOMOTORS;
    use crate::zero_hour::format::ini::object_ini::samples::{
        DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS,
    };
    use crate::zero_hour::format::ini::player_template_ini::samples::PLAYER_TEMPLATES;

    #[test]
    fn a_skirmish_starts_each_playable_faction_with_its_objects_and_offset() {
        let start = |templates: &str| {
            let mut rules = ZeroHourRules::default();
            for objects in [DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS] {
                rules.objects.read(objects.as_bytes()).unwrap();
            }
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
}
