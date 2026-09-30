use std::error::Error;
use std::fmt;

/// Why a unit type does not load.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitTypeError {
    /// The match's unit types would have more than 64 tags.
    TooManyTags,
    /// The match would have more unit types than a `u16` counts.
    TooManyTypes,
    /// Another unit type of the match has the name.
    RepeatedName,
}

impl fmt::Display for UnitTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            UnitTypeError::TooManyTags => "more than 64 unit tags",
            UnitTypeError::TooManyTypes => "more unit types than a u16 counts",
            UnitTypeError::RepeatedName => "another unit type has the name",
        })
    }
}

impl Error for UnitTypeError {}
