use std::error::Error;
use std::fmt;

/// Why points make no simple polygon. Edges count from 0, each from its point to the next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolygonError {
    TooFewPoints,
    /// Edges `first` and `second` meet where a simple polygon's do not.
    EdgesMeet {
        first: usize,
        second: usize,
    },
}

impl fmt::Display for PolygonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PolygonError::TooFewPoints => f.write_str("a polygon needs at least three points"),
            PolygonError::EdgesMeet { first, second } => {
                write!(f, "edges {first} and {second} of the polygon cross")
            }
        }
    }
}

impl Error for PolygonError {}
