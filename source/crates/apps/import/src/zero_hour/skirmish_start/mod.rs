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
    pub(crate) fn of<'r>(
        rules: &'r ZeroHourRules,
        names: &mut ImportNames<'r>,
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
    fn of<'r>(
        rules: &'r ZeroHourRules,
        names: &mut ImportNames<'r>,
        template_name: &str,
        template: &PlayerTemplate,
    ) -> Result<Self, ZeroHourError> {
        let ini = &rules.objects;
        let no_start = || ZeroHourError::NoStart(template_name.to_owned());
        let building = template.building.as_deref().ok_or_else(no_start)?;
        let object = |name: &str| {
            ini.object(name.as_bytes())
                .ok_or_else(|| ZeroHourError::NoStartObject(name.to_owned()))
        };
        let mut unit_type = |object: &'r IniObject| {
            names
                .unit_type(object, rules)
                .map(|named| DeclaredName::from(named.name))
                .map_err(|error| Self::type_error(object, error))
        };
        let building = object(building)?;
        let half = building.geometry().bounding_sphere() / 2.0;
        let offset =
            game_float::to_num(half).map_err(|_| Self::type_error(building, TypeError::Number))?;
        let faction = template_name.strip_prefix(LEAD).unwrap_or(template_name);
        Ok(Self {
            name: ImportName::of(faction.as_bytes()).declared()?,
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
mod tests;
