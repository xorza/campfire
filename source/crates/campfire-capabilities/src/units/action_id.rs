use serde::{Deserialize, Serialize};

use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::name_script_arg;
use crate::values::names::NameId;

/// An action, by its place in the match's action book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ActionId(u32);

impl ActionId {
    /// The action at `index` of the book.
    pub(crate) const fn new(index: u32) -> ActionId {
        ActionId(index)
    }

    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for ActionId {
    type Scope = u16;
    const LIMIT: usize = 1 << u32::BITS;

    fn at(index: usize) -> ActionId {
        ActionId(u32::try_from(index).expect("actions fit u32"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

// An ability a script names is of the script's package.
name_script_arg!(ActionId, NameKind::Ability, |ctx, name| {
    let package = ctx.frame().package();
    ctx.view().names().actions.named((package, name))
});
