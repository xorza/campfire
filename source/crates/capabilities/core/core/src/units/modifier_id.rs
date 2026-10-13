use campfire_values::NameId;
use serde::{Deserialize, Serialize};

use crate::name_script_arg;
use crate::scripts::name_kind::NameKind;

/// A modifier, by its place in the match's modifier book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModifierId(u16);

impl ModifierId {
    /// The modifier at `index` of the book.
    pub const fn new(index: u16) -> ModifierId {
        ModifierId(index)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for ModifierId {
    type Scope = u16;
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> ModifierId {
        ModifierId(u16::try_from(index).expect("modifiers fit u16"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

// A modifier a script names is of the script's package.
name_script_arg!(ModifierId, NameKind::Modifier, |ctx, name| {
    let package = ctx.frame().package();
    ctx.view().names().modifiers.named((package, name))
});
