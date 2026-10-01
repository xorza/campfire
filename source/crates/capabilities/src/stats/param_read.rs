use campfire_math::Num;

use crate::stats::live_param::LiveParam;

/// A param as a modifier's number read it: its value, and when it is a scaling table, which
/// param to read again as its source changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ParamRead {
    pub(crate) value: Num,
    pub(crate) live: Option<LiveParam>,
}
