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
