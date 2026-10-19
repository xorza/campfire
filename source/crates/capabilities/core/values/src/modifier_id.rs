use serde::{Deserialize, Serialize};

use crate::name_kind::{Article, NameKind, NameScope, Named};
use crate::names::NameId;
use crate::package_index::PackageIndex;

/// A modifier, by its place in the match's modifier book.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ModifierId(u16);

impl NameId for ModifierId {
    type Scope = PackageIndex;
    const LIMIT: usize = 1 << u16::BITS;

    fn at(index: usize) -> Self {
        Self(u16::try_from(index).expect("modifiers fit u16"))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

impl Named for ModifierId {
    const NAMES: NameKind = NameKind::new("modifier", Article::A, NameScope::Package);
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use crate::modifier_id::ModifierId;

    impl ModifierId {
        /// The modifier at `index` of the book.
        pub const fn new(index: u16) -> Self {
            Self(index)
        }
    }
}
