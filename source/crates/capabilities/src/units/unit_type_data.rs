use std::collections::BTreeMap;

use serde::Deserialize;

use crate::units::tag::Tag;
use crate::values::declared_name::DeclaredName;
use crate::values::scalar::Scalar;

/// A unit type's core fields as its data file declares them: its tags, which filters select, and
/// the params its scripts read as `unit.params`. Each capability the type uses reads its own
/// section.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct UnitTypeData {
    #[serde(default)]
    pub tags: Vec<DeclaredName>,
    #[serde(default)]
    pub params: BTreeMap<DeclaredName, Scalar>,
}

impl UnitTypeData {
    /// The most tags the unit types of a match declare together.
    pub const TAG_LIMIT: usize = Tag::LIMIT;
    /// The most unit types a match loads.
    pub const TYPE_LIMIT: usize = 1 << u16::BITS;
}

#[cfg(test)]
pub(crate) mod internals {
    use crate::units::unit_type_data::UnitTypeData;
    use crate::values::declared_name::DeclaredName;

    impl UnitTypeData {
        /// A type of `tags` and no params.
        pub(crate) fn tagged(tags: &[&str]) -> UnitTypeData {
            UnitTypeData {
                tags: tags
                    .iter()
                    .map(|&tag| DeclaredName::new(tag).unwrap())
                    .collect(),
                ..UnitTypeData::default()
            }
        }
    }
}
