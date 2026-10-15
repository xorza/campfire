use serde::{Deserialize, Serialize};

use crate::names::NameId;

/// A player resource, by its place in the mode's `resources`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ResourceId(u8);

impl ResourceId {
    /// Its place in the mode's `resources`.
    pub const fn get(self) -> u8 {
        self.0
    }
}

impl NameId for ResourceId {
    type Scope = ();
    /// The most player resources a mode declares: every index a `u8` holds.
    const LIMIT: usize = 1 << u8::BITS;

    fn at(index: usize) -> ResourceId {
        ResourceId(u8::try_from(index).expect("resources fit u8"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::resource_id::ResourceId;

    impl ResourceId {
        /// The resource at `index` of the mode's.
        pub const fn new(index: u8) -> ResourceId {
            ResourceId(index)
        }
    }
}
