use thiserror::Error;

use crate::zero_hour::format::error::Short;

/// Why a string table's bytes are none the game reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum CsfError {
    /// It runs past its end.
    #[error("it runs past its end")]
    Short,
    /// It does not start with the tag `CSF `, or gives a length below 0.
    #[error("it is no string table")]
    NotCsf,
    /// A label, by its place from 0, does not start with the tag `LBL `.
    #[error("label {index} has no label's tag")]
    NotLabel { index: usize },
    /// A string of a label, by the label's place, starts with neither `STR ` nor `STRW`, or gives
    /// a length below 0.
    #[error("a string of label {index} has no string's tag")]
    NotString { index: usize },
    /// A string of a label, by the label's place, is no UTF-16 text.
    #[error("a string of label {index} is no text")]
    NotText { index: usize },
}

impl From<Short> for CsfError {
    fn from(_: Short) -> Self {
        Self::Short
    }
}
