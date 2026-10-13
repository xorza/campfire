use campfire_values::NameId;
use serde::{Deserialize, Serialize};

use crate::name_script_arg;
use crate::scripts::name_kind::NameKind;

/// A player resource, by its place in the mode's `resources`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourceId(u8);

impl ResourceId {
    /// The most player resources a mode declares: every index a `u8` holds.
    pub const LIMIT: usize = 256;

    pub const fn index(self) -> usize {
        self.0 as usize
    }
}

impl NameId for ResourceId {
    type Scope = ();
    const LIMIT: usize = ResourceId::LIMIT;

    fn at(index: usize) -> ResourceId {
        ResourceId(u8::try_from(index).expect("resources fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

name_script_arg!(ResourceId, NameKind::Resource, |ctx, name| {
    ctx.view().names().resources.named(name)
});

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::units::resource_id::ResourceId;

    impl ResourceId {
        /// The resource at `index` of the mode's.
        pub const fn nth(index: u8) -> ResourceId {
            ResourceId(index)
        }
    }
}
