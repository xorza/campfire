/// Why a player's input does not count, which the log refuses as an `InputError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StampError {
    /// It is stamped before the input before it.
    Back,
    /// It takes the inputs of one stamp past the max inputs per tick.
    TooMany,
}
