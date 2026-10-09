use thiserror::Error;

/// A time of data, an action's or a unit type's, too large to count in ticks at the match's rate,
/// which only the rate decides, past the package load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("time too large to count in ticks")]
pub struct TimeTooLarge;

/// Why a binary file of a package, such as a map's heights, does not decode. Packages are
/// untrusted, so each is an expected failure.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DecodeError {
    /// Its bytes are no value of its type, or one its checks refuse.
    #[error("it does not decode")]
    Malformed(#[source] postcard::Error),
    /// Its bytes decode, but are not the one encoding of what they hold: postcard's decoder
    /// takes an over-long integer and ignores trailing bytes.
    #[error("it is not in its canonical encoding")]
    NotCanonical,
}
