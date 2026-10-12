use bevy_ecs::entity::Entity;

/// Where a restored state breaks a rule that a module other than its type's owner checks: on one
/// entity, or in the match as a whole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Breach {
    Entity(Entity),
    Match,
}

impl Breach {
    /// `Match` when `holds` is false: a rule of the whole match.
    pub const fn of_match(holds: bool) -> Result<(), Breach> {
        if holds { Ok(()) } else { Err(Breach::Match) }
    }
}
