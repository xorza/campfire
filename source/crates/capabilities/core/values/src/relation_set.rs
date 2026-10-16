use campfire_common::NamedEnum;

use crate::relation::Relation;

campfire_common::enum_all! {
    /// Which units a filter selects, by how a unit's team regards theirs: its enemies, which it may
    /// attack, hostile and neutral; only the hostile; only the neutral; its allies, the friendly,
    /// which include itself; or all.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RelationSet {
        Enemies,
        Hostiles,
        Neutrals,
        Allies,
        All,
    }
    const ALL;
}

/// As filters write each.
impl NamedEnum for RelationSet {
    const ALL: &'static [Self] = &Self::ALL;
    const NAMES: &'static [&'static str] = &["enemies", "hostiles", "neutrals", "allies", "all"];
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
