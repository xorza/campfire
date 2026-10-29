use campfire_common::DeclaredName;
use campfire_names::UnknownName;
use campfire_values::TimeTooLarge;
use thiserror::Error;

/// Why an action's area delivery or launch breaks a rule of the areas. Packages are untrusted, so
/// each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AreaDeliveryError {
    /// It delivers or launches a unit type with no `area` section.
    #[error("unit type {0} lacks the area section its delivery needs")]
    NotArea(DeclaredName),
    /// It aims along a direction, and an area lands on a point, a unit or the caster.
    #[error("an area lands on a point, a unit or the caster, not along a direction")]
    Direction,
}

/// Why an area type's data does not load into a match: a time too large to count in ticks at
/// its rate, or a name the match does not have.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AreaDataError {
    #[error(transparent)]
    Time(#[from] TimeTooLarge),
    #[error(transparent)]
    Unknown(#[from] UnknownName),
}
