use campfire_common::DeclaredName;
use thiserror::Error;

/// Why a use, a `use` section or the data that names a use breaks a rule of interaction.
/// Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UseError {
    /// A use that aims at something but a unit its filter selects.
    #[error("a use aims at a unit its filter selects")]
    Aims,
    /// A `use` section names an action of another kind.
    #[error("its use names action {0}, which is not of the `use` kind")]
    NotUse(DeclaredName),
    /// Data other than a `use` section names a use: a unit type's slots or a loadout.
    #[error("names action {0}, a use, which only a `use` section names")]
    Named(DeclaredName),
}

/// Why an entry, an `enter` or a `rides` section, or the data that names an entry, breaks a rule
/// of interaction. Packages are untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EnterError {
    /// An entry that aims at something but a unit its filter selects.
    #[error("an entry aims at a unit its filter selects")]
    Aims,
    /// An `enter` section names an action of another kind.
    #[error("its enter names action {0}, which is not of the `enter` kind")]
    NotEnter(DeclaredName),
    /// Data other than an `enter` section names an entry: a unit type's slots or a loadout.
    #[error("names action {0}, an entry, which only an `enter` section names")]
    Named(DeclaredName),
    /// An `enter` section of no capacity, which takes no rider.
    #[error("its enter holds no rider")]
    NoCapacity,
    /// An `enter` section whose payload is of a type that does not ride, or past its capacity.
    #[error("its enter's payload does not ride in it")]
    Payload,
    /// An `enter` section's fire point past the bound of a place, or past the 256 points a
    /// rider's place counts.
    #[error("its enter's fire point lies past the bound of a place")]
    FirePoint,
    /// A `rides` section on a type of no `uses`, whose entry has no slot to run from.
    #[error("it rides, and has no use slot to enter by")]
    RidesWithoutUses,
}
