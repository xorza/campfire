use serde::{Deserialize, Serialize};

use crate::scripts::name_kind::NameKind;
use crate::scripts::script_arg::name_script_arg;
use crate::values::names::NameId;

/// A path of the map, by its place in the map's list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PathId(u32);

impl PathId {
    pub(crate) const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for PathId {
    type Scope = ();
    const LIMIT: usize = 1 << u32::BITS;

    fn at(index: usize) -> PathId {
        PathId(u32::try_from(index).expect("paths fit u32"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

name_script_arg!(PathId, NameKind::Path, |ctx, name| {
    ctx.view().names().paths.named(name)
});

#[cfg(test)]
pub(crate) mod internals {
    use crate::units::path_id::PathId;

    impl PathId {
        /// The path at `index` of the map's.
        pub(crate) const fn new(index: u32) -> PathId {
            PathId(index)
        }
    }
}
