use campfire_math::Num;
use campfire_values::DeclaredName;

use crate::zero_hour::error::{TypeError, ZeroHourError};
use crate::zero_hour::game_float;
use crate::zero_hour::import_names::ImportNames;
use crate::zero_hour::locomotor_ini::LocomotorIni;
use crate::zero_hour::object_ini::{IniObject, ObjectIni};
use crate::zero_hour::player_template_ini::PlayerTemplateIni;

/// The player template a skirmish starts its players as, until a faction choice comes (design
/// 17, D6).
const FACTION: &str = "FactionAmerica";

/// What a skirmish starts each player with, which the mode's params give its script (design 17,
/// D6): its faction's building and starting units, by their unit types, and how far along `+z`
/// from the building its units start on a map with no rally, half the building's bounding
/// sphere, as `placeNetworkBuildingsForPlayer` moves them toward the original's `−y`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SkirmishStart {
    pub(crate) building: DeclaredName,
    pub(crate) units: Vec<DeclaredName>,
    pub(crate) offset: Num,
}

impl SkirmishStart {
    /// The start of `FACTION` in `templates`, its objects of `ini` unit types `names` takes, of
    /// `locomotors`; an error for a template or an object the install lacks, or an object that
    /// gives no unit type.
    pub(crate) fn of(
        templates: &PlayerTemplateIni,
        ini: &ObjectIni,
        locomotors: &LocomotorIni,
        names: &mut ImportNames,
    ) -> Result<Self, ZeroHourError> {
        let no_start = || ZeroHourError::NoStart(FACTION.to_owned());
        let template = templates.get(FACTION).ok_or_else(no_start)?;
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
        Ok(Self {
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
    use crate::zero_hour::locomotor_ini::internals::LOCOMOTORS;
    use crate::zero_hour::object_ini::internals::{DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS};
    use crate::zero_hour::player_template_ini::internals::PLAYER_TEMPLATES;

    #[test]
    fn a_skirmish_starts_with_its_factions_objects_and_offset() {
        let mut ini = ObjectIni::default();
        for objects in [DEFAULT_OBJECTS, MORE_OBJECTS, OBJECTS] {
            ini.read(objects.as_bytes()).unwrap();
        }
        let mut locomotors = LocomotorIni::default();
        locomotors.read(LOCOMOTORS.as_bytes()).unwrap();
        let start = |templates: &str| {
            let mut read = PlayerTemplateIni::default();
            read.read(templates.as_bytes()).unwrap();
            SkirmishStart::of(&read, &ini, &locomotors, &mut ImportNames::default())
        };
        // America's command center, of a sphere of radius 25, and its dozer, 12.5 m along +z.
        let name = |name: &str| DeclaredName::new(name).unwrap();
        assert_eq!(
            start(PLAYER_TEMPLATES).unwrap(),
            SkirmishStart {
                building: name("americacommandcenter"),
                units: vec![name("americavehicledozer")],
                offset: Num::int(25) / Num::int(2),
            }
        );
        // No America, an America of no building, or of objects the INI files lack, is refused.
        for templates in [
            "PlayerTemplate FactionChina\nEnd\n",
            "PlayerTemplate FactionAmerica\nEnd\n",
        ] {
            let refused = start(templates);
            assert!(
                matches!(&refused, Err(ZeroHourError::NoStart(name)) if name == FACTION),
                "{refused:?}"
            );
        }
        for templates in [
            "PlayerTemplate FactionAmerica\n StartingBuilding = Lost\nEnd\n",
            "PlayerTemplate FactionAmerica\n StartingBuilding = AmericaCommandCenter\n \
             StartingUnit0 = Lost\nEnd\n",
        ] {
            let refused = start(templates);
            assert!(
                matches!(&refused, Err(ZeroHourError::NoStartObject(name)) if name == "Lost"),
                "{refused:?}"
            );
        }
    }
}
