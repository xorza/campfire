use std::collections::BTreeMap;

use campfire_script::SyncedStateDecl;
use campfire_values::{DeclaredName, Scalar, Tag};
use serde::Deserialize;

/// A unit type's core fields as its data file declares them: its tags, which filters select, the
/// params its scripts read as `unit.params`, and the script state its units hold, `unit.state`.
/// Each capability the type uses reads its own section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct UnitTypeCore {
    #[serde(default)]
    pub tags: Vec<DeclaredName>,
    #[serde(default)]
    pub params: BTreeMap<DeclaredName, Scalar>,
    #[serde(default)]
    pub state: BTreeMap<DeclaredName, SyncedStateDecl>,
}

impl UnitTypeCore {
    /// The most tags the unit types of a match declare together.
    pub const TAG_LIMIT: usize = Tag::LIMIT;
    /// The most unit types a match loads.
    pub const TYPE_LIMIT: usize = 1 << u16::BITS;
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use campfire_values::{DeclaredName, Scalar};

    use crate::unit_type_core::UnitTypeCore;

    impl UnitTypeCore {
        /// A type of `tags` and no params.
        pub fn tagged(tags: &[&str]) -> UnitTypeCore {
            UnitTypeCore::of(tags, &[])
        }

        /// A type of `tags` and `params`.
        pub fn of(tags: &[&str], params: &[(&str, Scalar)]) -> UnitTypeCore {
            let name = |name| DeclaredName::new(name).unwrap();
            UnitTypeCore {
                tags: tags.iter().map(|&tag| name(tag)).collect(),
                params: params
                    .iter()
                    .map(|&(key, value)| (name(key), value))
                    .collect(),
                state: BTreeMap::new(),
            }
        }
    }
}
