use thiserror::Error;

/// Why a point is not a `Position`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum OutOfBounds {
    /// A coordinate lies beyond ±`Position::BOUND`.
    #[error("a coordinate beyond the world bound of 2^20 m")]
    Beyond,
}
