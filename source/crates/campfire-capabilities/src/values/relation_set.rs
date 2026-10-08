use crate::values::relation::Relation;

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

impl RelationSet {
    const ALL: [RelationSet; 5] = [
        RelationSet::Enemies,
        RelationSet::Hostiles,
        RelationSet::Neutrals,
        RelationSet::Allies,
        RelationSet::All,
    ];

    /// A filter's relation by its name.
    pub fn named(name: &str) -> Option<RelationSet> {
        RelationSet::ALL
            .into_iter()
            .find(|relation| relation.name() == name)
    }

    /// The relation as filters write it.
    pub const fn name(self) -> &'static str {
        match self {
            RelationSet::Enemies => "enemies",
            RelationSet::Hostiles => "hostiles",
            RelationSet::Neutrals => "neutrals",
            RelationSet::Allies => "allies",
            RelationSet::All => "all",
        }
    }

    /// Whether it selects a unit of a team regarded with `relation`.
    pub const fn selects(self, relation: Relation) -> bool {
        match self {
            RelationSet::Enemies => relation.may_attack(),
            RelationSet::Hostiles => matches!(relation, Relation::Hostile),
            RelationSet::Neutrals => matches!(relation, Relation::Neutral),
            RelationSet::Allies => matches!(relation, Relation::Friendly),
            RelationSet::All => true,
        }
    }
}
