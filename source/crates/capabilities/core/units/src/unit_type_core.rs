use std::collections::BTreeMap;

use campfire_common::DeclaredName;
use campfire_declare::{Section, SectionTable, UnitTypeData};
use campfire_script::SyncedStateDecl;
use campfire_values::Scalar;
use serde::Deserialize;

/// A unit type's core fields as its data file declares them: its tags, which filters select, the
/// params its scripts read as `unit.params`, and the script state its units hold, `unit.state`.
/// Each capability the type uses reads its own section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UnitTypeCore {
    pub tags: Vec<DeclaredName>,
    pub params: BTreeMap<DeclaredName, Scalar>,
    pub state: BTreeMap<DeclaredName, SyncedStateDecl>,
}

/// A unit type's `tags`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct TypeTagsData(pub Vec<DeclaredName>);

impl Section for TypeTagsData {
    const TABLE: SectionTable = SectionTable::AnyType;
    const KEY: &'static str = "tags";
}

/// A unit type's `params`, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct TypeParamsData(pub BTreeMap<DeclaredName, Scalar>);

impl Section for TypeParamsData {
    const TABLE: SectionTable = SectionTable::AnyType;
    const KEY: &'static str = "params";
}

/// A unit type's `state`, each field by name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct TypeStateData(pub BTreeMap<DeclaredName, SyncedStateDecl>);

impl Section for TypeStateData {
    const TABLE: SectionTable = SectionTable::AnyType;
    const KEY: &'static str = "state";
}

/// The core fields of a unit type's sections, each empty where it gives none.
impl From<&UnitTypeData> for UnitTypeCore {
    fn from(data: &UnitTypeData) -> Self {
        Self {
            tags: data
                .section::<TypeTagsData>()
                .map_or_else(Vec::new, |tags| tags.0.clone()),
            params: data
                .section::<TypeParamsData>()
                .map_or_else(BTreeMap::new, |params| params.0.clone()),
            state: data
                .section::<TypeStateData>()
                .map_or_else(BTreeMap::new, |state| state.0.clone()),
        }
    }
}

#[cfg(feature = "internals")]
pub(crate) mod internals {
    use std::collections::BTreeMap;

    use campfire_common::DeclaredName;
    use campfire_values::Scalar;

    use crate::unit_type_core::UnitTypeCore;

    impl UnitTypeCore {
        /// A type of `tags` and no params.
        pub fn tagged(tags: &[&str]) -> Self {
            Self::of(tags, &[])
        }

        /// A type of `tags` and `params`.
        pub fn of(tags: &[&str], params: &[(&str, Scalar)]) -> Self {
            let name = |name| DeclaredName::new(name).unwrap();
            Self {
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
