use campfire_common::DeclaredName;

use crate::zero_hour::error::NameError;
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::{
    IniUpgradeModule, UpgradeEffect,
};
use crate::zero_hour::import_name::ImportName;

/// The tag the look of `module`'s object tests for its effect, which the module's modifier gives
/// (09-zero-hour.md, An upgrade's look is a tag): the name of the model condition it sets, as
/// its object's states name it, `power_plant_upgraded` of a `PowerPlantUpgrade`, which sets
/// `POWER_PLANT_UPGRADED`, and its `ConditionFlag` of a `ModelConditionUpgrade`; and of a
/// `SubObjectsUpgrade`, which sets no condition, `<first trigger>_subobjects`, which its object's
/// models name, and of a `CommandSetUpgrade`, `<first trigger>_commandset`, which its object's
/// panel of the set names. None for a module of another effect, of no flag, of no set or of no
/// trigger; an error past the bytes a name holds.
pub(crate) fn of(module: &IniUpgradeModule) -> Result<Option<DeclaredName>, NameError> {
    let name = match &module.effect {
        UpgradeEffect::PowerPlant => ImportName::latin("power_plant_upgraded"),
        UpgradeEffect::ModelCondition { flag: Some(flag) } => ImportName::latin(flag),
        UpgradeEffect::SubObjects { .. } => match module.triggers.first() {
            Some(first) => return ImportName::latin(first).suffixed("subobjects").map(Some),
            None => return Ok(None),
        },
        UpgradeEffect::CommandSet { set: Some(_), .. } => match module.triggers.first() {
            Some(first) => return ImportName::latin(first).suffixed("commandset").map(Some),
            None => return Ok(None),
        },
        _ => return Ok(None),
    };
    name.declared().map(Some)
}
