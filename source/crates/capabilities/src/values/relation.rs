/// Which units a filter selects, relative to a unit: its enemies, its allies, which include
/// itself, or all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Enemies,
    Allies,
    All,
}

impl Relation {
    const ALL: [Relation; 3] = [Relation::Enemies, Relation::Allies, Relation::All];

    /// A filter's relation: `enemies`, `allies` or `all`.
    pub fn named(name: &str) -> Option<Relation> {
        Relation::ALL
            .into_iter()
            .find(|relation| relation.name() == name)
    }

    /// The relation as filters write it.
    pub const fn name(self) -> &'static str {
        match self {
            Relation::Enemies => "enemies",
            Relation::Allies => "allies",
            Relation::All => "all",
        }
    }
}
