use crate::combat::team::Team;

/// Which units a filter selects, relative to a unit: its enemies, its allies, which include
/// itself, or all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Enemies,
    Allies,
    All,
}

impl Relation {
    /// A filter's relation: `enemies`, `allies` or `all`.
    pub fn parse(text: &str) -> Option<Relation> {
        match text {
            "enemies" => Some(Relation::Enemies),
            "allies" => Some(Relation::Allies),
            "all" => Some(Relation::All),
            _ => None,
        }
    }

    /// Whether a unit of team `theirs` stands in this relation to one of team `ours`.
    pub(crate) const fn holds(self, ours: Team, theirs: Team) -> bool {
        match self {
            Relation::Enemies => ours.is_enemy_of(theirs),
            Relation::Allies => !ours.is_enemy_of(theirs),
            Relation::All => true,
        }
    }
}
