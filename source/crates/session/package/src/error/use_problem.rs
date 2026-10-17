use campfire_common::DeclaredName;
use thiserror::Error;

/// What does not hold of the uses: a `use` action, a `use` section, or a `uses` section.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum UseProblem {
    /// A use aims at something but a unit its filter selects.
    #[error("action \"{0}\": a use aims at a unit its filter selects")]
    Aims(DeclaredName),
}
