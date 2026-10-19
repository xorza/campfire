//! The script arguments of the ids `values` holds, each resolved by name in the call's view.

use campfire_values::{
    ActionId, DamageKind, ModifierId, PathId, ResourceId, Tag, Team, TrackId, TypeScope, UnitType,
};

use crate::name_script_arg;

// An ability a script names is of the script's package.
name_script_arg!(ActionId, |ctx, name| {
    let package = ctx.frame().package();
    ctx.view().names().named((package, name))
});

// A modifier a script names is of the script's package.
name_script_arg!(ModifierId, |ctx, name| {
    let package = ctx.frame().package();
    ctx.view().names().named((package, name))
});

name_script_arg!(PathId, |ctx, name| { ctx.view().names().named(name) });

name_script_arg!(TrackId, |ctx, name| { ctx.view().names().named(name) });

name_script_arg!(ResourceId, |ctx, name| { ctx.view().names().named(name) });

name_script_arg!(Tag, |ctx, name| { ctx.view().names().tags().named(name) });

name_script_arg!(Team, |ctx, name| { ctx.view().names().named(name) });

name_script_arg!(DamageKind, |ctx, name| { ctx.view().names().named(name) });

// A unit type a script names is of the mode's scope: one of the mode's, or an avatar.
name_script_arg!(UnitType, |ctx, name| {
    ctx.view().names().named((TypeScope::Mode, name))
});
