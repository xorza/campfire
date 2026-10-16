use thiserror::Error;

/// Why a `RefPack` stream does not decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum RefPackError {
    /// It ends inside its header, a command or its literal bytes.
    #[error("it ends inside a command")]
    Short,
    /// A copy reaches back before the first byte written.
    #[error("a copy reaches before the start")]
    BeforeStart,
    /// It writes more bytes than its header gives.
    #[error("it writes more than its {0} bytes")]
    TooLong(usize),
    /// It ends having written fewer bytes than its header gives.
    #[error("it writes {written} of its {len} bytes")]
    TooShort { written: usize, len: usize },
}
