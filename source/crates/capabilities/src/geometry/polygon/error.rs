use thiserror::Error;

/// Why points make no simple polygon. Edges count from 0, each from its point to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum PolygonError {
    #[error("a polygon needs at least three points")]
    TooFewPoints,
    /// Edges `first` and `second` meet where a simple polygon's do not.
    #[error("edges {first} and {second} of the polygon cross")]
    EdgesMeet { first: usize, second: usize },
}
