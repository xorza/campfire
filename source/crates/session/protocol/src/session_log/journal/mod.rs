use std::mem;

use campfire_common::PlayerSlot;

use crate::session_log::SessionLog;
use crate::session_log::durable_head::DurableHead;
use crate::session_log::durable_heads::DurableHeads;
use crate::session_log::journal::journal_frames::JournalFrames;
use crate::session_log::journal::record_sink::RecordSink;
use crate::session_log::journal_shape::JournalShape;

/// A server's journal of its session's log: the sink its records go to, the frames of the records
/// the log took since the sink last took them, how many records the log took, how many bytes the
/// journal's file holds, and where each player slot's chain stands in the records the sink synced.
/// Only a server's session holds one, and gives it to each call of its log that takes a record
/// (08-sessions.md, The journal belongs to the server's session). A drop hands the sink what the
/// journal holds.
#[derive(Debug)]
pub struct Journal {
    sink: Box<dyn RecordSink>,
    /// The frames the sink takes at the next `flush`, in order.
    frames: Vec<u8>,
    /// How many records `frames` holds.
    framed: u64,
    /// The records the log took into the journal since it was given the sink.
    written: u64,
    /// The bytes of the file: the tag and every frame, those before the log was given the sink
    /// and those not flushed yet among them.
    len: u64,
    durable: DurableHeads,
    /// The shape of the records the journal holds, kept as it appends, so a publication reads
    /// its file once; boxed, as a session holds the journal in its role beside a verifier's, of
    /// none.
    shape: Box<JournalShape>,
}

impl Journal {
    /// The journal of `sink`, whose file holds `len` bytes so far, of a session of `player_slots`
    /// player slots.
    pub(crate) fn new(
        sink: Box<dyn RecordSink>,
        len: u64,
        player_slots: u32,
        shape: JournalShape,
    ) -> Self {
        let player_slots = usize::try_from(player_slots).expect("a u32 fits usize");
        Self {
            sink,
            frames: Vec::new(),
            framed: 0,
            written: 0,
            len,
            durable: DurableHeads::new(player_slots),
            shape: Box::new(shape),
        }
    }

    /// A new journal of `sink`, whose file holds the journal's tag alone, given the records of
    /// what `log` holds; the log's calls that take a record add each of theirs as they take it.
    pub fn start(log: &SessionLog, sink: Box<dyn RecordSink>) -> Self {
        let tag = u64::try_from(JournalFrames::TAG.len()).expect("the tag's length fits u64");
        let mut journal = Self::new(sink, tag, log.player_slot_count(), JournalShape::default());
        log.write_journal(&mut journal);
        journal
    }

    /// The journal of `sink`, whose file holds `len` bytes and every record `log` took, of the
    /// shape `shape`, as the log and the shape were rebuilt from it: every chain whose head its
    /// player's current delegation signed stands durably where the log holds it, as the journal
    /// was read back from the disk.
    pub fn resume(
        log: &SessionLog,
        sink: Box<dyn RecordSink>,
        len: u64,
        shape: JournalShape,
    ) -> Self {
        let mut journal = Self::new(sink, len, log.player_slot_count(), shape);
        log.resume_durable(&mut journal);
        journal
    }

    /// Frames `record` after the frames the sink takes at the next `flush`.
    pub(crate) fn append(&mut self, record: &[u8]) {
        self.shape
            .take(record)
            .expect("a record the log takes is one a shape reads");
        let start = journal_frames::open(&mut self.frames);
        self.frames.extend_from_slice(record);
        journal_frames::seal(&mut self.frames, start);
        let framed = self.frames.len() - start;
        self.framed += 1;
        self.written += 1;
        self.len += u64::try_from(framed).expect("a frame's length fits u64");
    }

    /// Hands the sink the frames of the records the log took since the last flush, as one call,
    /// so its writer wakes once for them (08-sessions.md, A record is encoded once).
    pub fn flush(&mut self) {
        if self.framed == 0 {
            return;
        }
        let frames = &self.frames;
        self.sink
            .append(self.framed, &mut |out| out.extend_from_slice(frames));
        self.frames.clear();
        self.framed = 0;
    }

    /// How many records the log took into the journal.
    pub const fn written(&self) -> u64 {
        self.written
    }

    /// The shape of the records it holds, the unflushed among them.
    pub const fn shape(&self) -> &JournalShape {
        &self.shape
    }

    /// Hands the sink the records not flushed, and ends the journal: the shape of what its file
    /// holds.
    pub fn close(mut self) -> JournalShape {
        self.flush();
        *mem::take(&mut self.shape)
    }

    /// How many bytes the journal's file holds.
    pub const fn file_len(&self) -> u64 {
        self.len
    }

    /// Where each player slot's chain stands in the records the sink synced.
    pub(crate) const fn durable_mut(&mut self) -> &mut DurableHeads {
        &mut self.durable
    }

    /// Notes where each player's chain stands once the first `durable` records the log took into
    /// the journal are synced, as the sink's writer counts them.
    pub fn advance_durable(&mut self, durable: u64) {
        debug_assert!(
            durable <= self.written,
            "durable records are records the log took into its journal"
        );
        self.durable.advance(durable);
    }

    /// Where `player_slot`'s player's chain stands in the records the sink synced, as of the last
    /// `advance_durable`; none before any, and since a player took the player slot, renewed their
    /// delegation, or left it.
    pub fn durable_head(&self, player_slot: PlayerSlot) -> Option<DurableHead> {
        self.durable.get(player_slot)
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        self.flush();
    }
}

pub(crate) mod error;
pub(crate) mod journal_frames;
pub(crate) mod record_sink;

#[cfg(test)]
pub(crate) mod tests;
