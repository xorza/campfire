//! The script arguments of the ids `values` holds, each resolved by name in the call's view.

use campfire_values::{
    ActionId, ModifierId, PathId, ResourceId, Tag, Team, TrackId, TypeScope, UnitType,
};

use crate::name_script_arg;
use crate::scripts::name_kind::NameKind;

// An ability a script names is of the script's package.
name_script_arg!(ActionId, NameKind::Ability, |ctx, name| {
    let package = ctx.frame().package();
    ctx.view().names().named((package, name))
});

// A modifier a script names is of the script's package.
name_script_arg!(ModifierId, NameKind::Modifier, |ctx, name| {
    let package = ctx.frame().package();
    ctx.view().names().named((package, name))
});

name_script_arg!(PathId, NameKind::Path, |ctx, name| {
    ctx.view().names().named(name)
});

name_script_arg!(TrackId, NameKind::Track, |ctx, name| {
    ctx.view().names().named(name)
});

name_script_arg!(ResourceId, NameKind::Resource, |ctx, name| {
    ctx.view().names().named(name)
});

name_script_arg!(Tag, NameKind::Tag, |ctx, name| {
    ctx.view().names().tags().named(name)
});

name_script_arg!(Team, NameKind::Team, |ctx, name| {
    ctx.view().names().named(name)
});

// A unit type a script names is of the mode's scope: one of the mode's, or an avatar.
name_script_arg!(UnitType, NameKind::UnitType, |ctx, name| {
    ctx.view().names().named((TypeScope::Mode, name))
});
