//! The byte that starts each kind of journal record.

pub(super) const JOURNAL_HEADER: u8 = 0;
pub(super) const JOURNAL_PACKET: u8 = 1;
pub(super) const JOURNAL_SERVER: u8 = 2;
pub(super) const JOURNAL_SEALED: u8 = 3;
pub(super) const JOURNAL_CHECKPOINT_DONE: u8 = 4;
pub(super) const JOURNAL_RESULT: u8 = 5;
pub(super) const JOURNAL_CHECKPOINT_BEGUN: u8 = 6;
pub(super) const JOURNAL_LOADED: u8 = 7;
