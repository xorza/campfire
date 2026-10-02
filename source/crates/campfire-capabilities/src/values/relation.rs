use crate::values::attitude::Attitude;

/// Which units a filter selects, by how a unit's team regards theirs: its enemies, which it may
/// attack, hostile and neutral; only the hostile; only the neutral; its allies, the friendly,
/// which include itself; or all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Enemies,
    Hostiles,
    Neutrals,
    Allies,
    All,
}

impl Relation {
    const ALL: [Relation; 5] = [
        Relation::Enemies,
        Relation::Hostiles,
        Relation::Neutrals,
        Relation::Allies,
        Relation::All,
    ];

    /// A filter's relation by its name.
    pub fn named(name: &str) -> Option<Relation> {
        Relation::ALL
            .into_iter()
            .find(|relation| relation.name() == name)
    }

    /// The relation as filters write it.
    pub const fn name(self) -> &'static str {
        match self {
            Relation::Enemies => "enemies",
            Relation::Hostiles => "hostiles",
            Relation::Neutrals => "neutrals",
            Relation::Allies => "allies",
            Relation::All => "all",
        }
    }

    /// Whether it selects a unit of a team regarded with `attitude`.
    pub const fn selects(self, attitude: Attitude) -> bool {
        match self {
            Relation::Enemies => attitude.may_attack(),
            Relation::Hostiles => matches!(attitude, Attitude::Hostile),
            Relation::Neutrals => matches!(attitude, Attitude::Neutral),
            Relation::Allies => matches!(attitude, Attitude::Friendly),
            Relation::All => true,
        }
    }
}
