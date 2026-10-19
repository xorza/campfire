use campfire_common::NamedEnum;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::relation::Relation;

/// Which units a filter selects, by how a unit's team regards theirs: its enemies, which it may
/// attack, hostile and neutral; only the hostile; only the neutral; its allies, the friendly,
/// which include itself; or all. Each member's name is as a filter writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, NamedEnum)]
pub enum RelationSet {
    Enemies,
    Hostiles,
    Neutrals,
    Allies,
    All,
}

impl RelationSet {
    /// Whether it selects a unit of a team regarded with `relation`.
    pub const fn selects(self, relation: Relation) -> bool {
        match self {
            Self::Enemies => relation.may_attack(),
            Self::Hostiles => matches!(relation, Relation::Hostile),
            Self::Neutrals => matches!(relation, Relation::Neutral),
            Self::Allies => matches!(relation, Relation::Friendly),
            Self::All => true,
        }
    }
}

/// As serde's derive writes a unit variant, by its name, as data writes a splash's `affects`.
impl Serialize for RelationSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.serialize_member(serializer)
    }
}

/// Read as serde's derive reads a unit variant, by its name, as data writes a splash's
/// `affects`.
impl<'de> Deserialize<'de> for RelationSet {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::deserialize_member(deserializer)
    }
}
