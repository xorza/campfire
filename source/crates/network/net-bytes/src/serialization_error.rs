/// Why bytes did not decode.
#[non_exhaustive]
#[derive(thiserror::Error, Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationError {
    /// Fewer bytes than the value takes.
    #[error("the bytes end before the value does")]
    Truncated,
    #[error("Invalid value")]
    InvalidValue,
    /// A value in an encoding longer than the one it takes.
    #[error("a value in an encoding other than its one encoding")]
    NotCanonical,
}
