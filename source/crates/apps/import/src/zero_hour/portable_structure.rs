use campfire_common::DeclaredName;

use crate::zero_hour::error::NameError;
use crate::zero_hour::format::ini::creation_list_ini::CreationNugget;
use crate::zero_hour::format::ini::object_ini::IniObject;
use crate::zero_hour::format::ini::object_ini::ini_contain::ContainClass;
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::{
    IniUpgradeModule, UpgradeEffect,
};
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// The transport an upgrade module `module` of `rules` makes inside its object, as
/// `OCL_OverlordBattleBunker` makes the Overlord's battle bunker, a portable structure of a
/// `TransportContain` (`ContainInsideSourceObject`), whose riders the import gives its object
/// (04-capabilities/interaction.md, A bunker is an entry a tag opens); none for a module of no such
/// creation.
pub(crate) fn transport<'r>(
    rules: &'r ZeroHourRules,
    module: &IniUpgradeModule,
) -> Option<&'r IniObject> {
    let UpgradeEffect::ObjectCreation { list: Some(list) } = &module.effect else {
        return None;
    };
    let nuggets = rules.creation_lists.lists().get(list)?;
    nuggets.iter().find_map(|nugget| {
        let CreationNugget::Objects(creation) = nugget else {
            return None;
        };
        let name = creation.names.first().filter(|_| creation.inside)?;
        let object = rules.objects.object(name.as_bytes())?;
        let contain = object.contain()?;
        (contain.class == ContainClass::Transport).then_some(object)
    })
}

/// The tag the upgrade module `module` gives its object as it makes a transport inside it, its
/// first trigger's name suffixed `created`, which opens its object's entry; none for a module of
/// no trigger.
pub(crate) fn tag(module: &IniUpgradeModule) -> Result<Option<DeclaredName>, NameError> {
    match module.triggers.first() {
        Some(first) => ImportName::latin(first).suffixed("created").map(Some),
        None => Ok(None),
    }
}
