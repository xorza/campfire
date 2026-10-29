use campfire_common::DeclaredName;

use crate::zero_hour::error::NameError;
use crate::zero_hour::format::ini::creation_list_ini::CreationNugget;
use crate::zero_hour::format::ini::object_ini::ini_contain::ContainClass;
use crate::zero_hour::format::ini::object_ini::ini_object::IniObject;
use crate::zero_hour::format::ini::object_ini::ini_upgrade_module::{
    IniUpgradeModule, UpgradeEffect,
};
use crate::zero_hour::format::ini::object_ini::object_kinds::ObjectKind;
use crate::zero_hour::import_name::ImportName;
use crate::zero_hour::zero_hour_rules::ZeroHourRules;

/// The portable structure an upgrade module `module` of `rules` makes inside its object, as
/// `OCL_OverlordGattlingCannon` makes the Overlord's Gattling cannon (`ContainInsideSourceObject`),
/// which the import folds into its object (riders-and-upgrades.md, A portable structure is a part
/// of its carrier); none for a module of no such creation.
pub(crate) fn structure<'r>(
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
        object
            .kinds()
            .holds(ObjectKind::PortableStructure)
            .then_some(object)
    })
}

/// The transport an upgrade module `module` of `rules` makes inside its object, as
/// `OCL_OverlordBattleBunker` makes the Overlord's battle bunker, a portable structure of a
/// `TransportContain`, whose riders the import gives its object
/// (04-capabilities/interaction.md, A bunker is an entry a tag opens); none for a module of no such
/// creation.
pub(crate) fn transport<'r>(
    rules: &'r ZeroHourRules,
    module: &IniUpgradeModule,
) -> Option<&'r IniObject> {
    structure(rules, module).filter(|object| {
        object
            .contain()
            .is_some_and(|contain| contain.class == ContainClass::Transport)
    })
}

/// The tag the `PassengersFireUpgrade` module `module` gives its object, its first trigger's name
/// suffixed `fire`, while which its riders fire (riders-and-upgrades.md, A rider's fire follows its
/// container's tags); none for a module of no trigger.
pub(crate) fn fire_tag(module: &IniUpgradeModule) -> Result<Option<DeclaredName>, NameError> {
    trigger_tag(module, "fire")
}

/// The tag the upgrade module `module` gives its object as it makes a portable structure inside
/// it, its first trigger's name suffixed `created`, which opens its object's entry or arms its
/// structure's weapons; none for a module of no trigger.
pub(crate) fn tag(module: &IniUpgradeModule) -> Result<Option<DeclaredName>, NameError> {
    trigger_tag(module, "created")
}

/// The first trigger's name of `module` suffixed `suffix`; none for a module of no trigger.
fn trigger_tag(module: &IniUpgradeModule, suffix: &str) -> Result<Option<DeclaredName>, NameError> {
    module
        .triggers
        .first()
        .map(|first| ImportName::latin(first).suffixed(suffix))
        .transpose()
}
