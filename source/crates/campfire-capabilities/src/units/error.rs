use thiserror::Error;

/// Why the match's unit types or tags do not load. Packages are untrusted, so each is an expected
/// failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TypesError {
    /// More tags than a match holds.
    #[error("more tags than a match holds")]
    TooManyTags,
    /// More unit types than a match holds.
    #[error("more unit types than a match holds")]
    TooManyTypes,
    /// Two unit types of one scope by one name, as an avatar named as one of the mode's types.
    #[error("unit type {0} twice")]
    Repeated(String),
}
