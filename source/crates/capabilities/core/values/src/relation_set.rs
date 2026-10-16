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

impl RelationSet {
    /// A filter's relation by its name.
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|relation| relation.name() == name)
    }

    /// The relation as filters write it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Enemies => "enemies",
            Self::Hostiles => "hostiles",
            Self::Neutrals => "neutrals",
            Self::Allies => "allies",
            Self::All => "all",
        }
    }

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
