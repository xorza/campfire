//! The log's journal: the record of each thing the log takes, as it takes it, and the log
//! rebuilt from them.

use campfire_common::{PlayerSlot, Tick, Version};

use crate::control::Control;
use crate::session_log::applied::Applied;
use crate::session_log::durable_head::DurableHead;
use crate::session_log::error::{CheckpointError, JournalReplayError, LogError};
use crate::session_log::journal::Journal;
use crate::session_log::log_entries::Entry;
use crate::session_log::segments::{LoggedCheckpoint, SegmentStart};
use crate::session_log::{LoggedResult, SessionLog, Signatures, file, put, take};

/// The bytes that start each kind of journal record.
const JOURNAL_HEADER: u8 = 0;
pub(super) const JOURNAL_PACKET: u8 = 1;
pub(super) const JOURNAL_SERVER: u8 = 2;
pub(super) const JOURNAL_SEALED: u8 = 3;
const JOURNAL_CHECKPOINT_DONE: u8 = 4;
const JOURNAL_RESULT: u8 = 5;
const JOURNAL_CHECKPOINT_BEGUN: u8 = 6;
const JOURNAL_LOADED: u8 = 7;

impl SegmentStart {
    /// Writes the begin of the checkpoint that starts segment `segment`, and its record once it
    /// has one, into `journal`, with `out` as scratch.
    fn journal_whole(&self, journal: &mut Journal, segment: u32, out: &mut Vec<u8>) {
        out.clear();
        write_begun(out, segment, self.tick);
        journal.append(out);
        if let Some(logged) = &self.checkpoint {
            out.clear();
            logged.write(out);
            journal.append(out);
        }
    }
}

impl LoggedCheckpoint {
    /// Writes its journal record into `out`, as the checkpoint is done: its kind's byte, the
    /// record, and its signature, which the file holds as they follow the byte.
    pub(super) fn write(&self, out: &mut Vec<u8>) {
        out.push(JOURNAL_CHECKPOINT_DONE);
        self.record.encode(out);
        put(out, &self.signature);
    }
}

/// Writes into `out` the journal record of the begin of the checkpoint that starts segment
/// `segment` at `tick`.
pub(super) fn write_begun(out: &mut Vec<u8>, segment: u32, tick: Tick) {
    out.push(JOURNAL_CHECKPOINT_BEGUN);
    put(out, &segment);
    put(out, &tick);
}

/// Writes into `out` the journal record of `result`, as the session ends: its kind's byte, then
/// the result with its signature, which the file holds as its option of some.
pub(super) fn write_result(out: &mut Vec<u8>, result: &LoggedResult) {
    out.push(JOURNAL_RESULT);
    put(out, result);
}

/// Writes into `out` the journal record of a load of the save of segment `segment`.
pub(super) fn write_loaded(out: &mut Vec<u8>, segment: u32) {
    out.push(JOURNAL_LOADED);
    put(out, &segment);
}

impl SessionLog {
    /// The log of the session whose journal holds `records`, in order, as the server took them:
    /// what the server had logged when it wrote the last. An error for a first record that is
    /// not the header, a header of a release other than `release`, the one that reads it, whose
    /// records are then not read, as their format is that release's, a record after the result,
    /// a checkpoint's begin after an entry of its tick, and a record that does not decode or
    /// that the log refuses.
    pub fn from_journal<'a>(
        records: impl IntoIterator<Item = &'a [u8]>,
        release: Version,
    ) -> Result<Self, JournalReplayError> {
        let failed = |record, error| JournalReplayError::Record { record, error };
        let mut records = records.into_iter();
        let first = records.next().ok_or(JournalReplayError::NoHeader)?;
        let mut rest = first
            .strip_prefix(&[JOURNAL_HEADER])
            .ok_or(JournalReplayError::NoHeader)?;
        let header = file::take_header(&mut rest).map_err(|error| failed(0, error))?;
        if !rest.is_empty() {
            return Err(failed(0, LogError::Trailing));
        }
        if header.terms.release != release {
            return Err(JournalReplayError::OtherRelease(header.terms.release));
        }
        let mut log = Self::new(header).map_err(|error| failed(0, LogError::Header(error)))?;
        let mut inputs = Vec::new();
        let mut applied = Vec::new();
        for (index, record) in (1..).zip(records) {
            if log.result.is_some() {
                return Err(JournalReplayError::AfterResult { record: index });
            }
            if record.first() == Some(&JOURNAL_CHECKPOINT_BEGUN) && !log.at_boundary() {
                return Err(JournalReplayError::CheckpointMidTick { record: index });
            }
            log.apply_journal(record, &mut inputs, &mut applied)
                .map_err(|error| failed(index, error))?;
        }
        Ok(log)
    }

    /// Whether no entry was logged since the last tick was sealed: where a checkpoint may come.
    pub(super) fn at_boundary(&self) -> bool {
        self.entries.at_boundary()
    }

    /// Takes the journal record `record`, past the header, with `inputs` and `applied` as
    /// scratch.
    fn apply_journal<'a>(
        &mut self,
        record: &'a [u8],
        inputs: &mut Vec<(Tick, &'a [u8])>,
        applied: &mut Vec<Applied>,
    ) -> Result<(), LogError> {
        let (&kind, mut rest) = record.split_first().ok_or(LogError::Truncated)?;
        match kind {
            JOURNAL_PACKET => self.record_packet(&mut rest, inputs, applied, Signatures::Check)?,
            JOURNAL_SERVER => self.record_server_input(&mut rest, Signatures::Check)?,
            JOURNAL_SEALED => drop(self.seal_tick(None)),
            JOURNAL_CHECKPOINT_BEGUN => {
                let segment: u32 = take(&mut rest)?;
                let tick: Tick = take(&mut rest)?;
                let begun = self
                    .begin_checkpoint(None)
                    .map_err(|error| LogError::Checkpoint { segment, error })?;
                if (begun.segment, begun.tick) != (segment, tick) {
                    let error = if begun.segment == segment {
                        CheckpointError::Tick
                    } else {
                        CheckpointError::Segment
                    };
                    return Err(LogError::Checkpoint { segment, error });
                }
            }
            JOURNAL_CHECKPOINT_DONE => {
                let segment = self
                    .segments
                    .oldest_begun()
                    .map_or_else(|| self.segment(), |begun| begun.segment);
                self.record_done(&mut rest, segment, Signatures::Check)?;
            }
            JOURNAL_LOADED => {
                let segment: u32 = take(&mut rest)?;
                self.load(segment, None)
                    .map_err(|error| LogError::Load { segment, error })?;
            }
            JOURNAL_RESULT => {
                let LoggedResult { result, signature } = take(&mut rest)?;
                self.record_result(result, &signature, None)
                    .map_err(LogError::Result)?;
            }
            _ => return Err(LogError::UnknownEntry),
        }
        if !rest.is_empty() {
            return Err(LogError::Trailing);
        }
        Ok(())
    }

    /// Writes into `journal`, a new one that holds the journal's tag alone, the records of what
    /// the log holds: the header, then each entry, each sealed tick, each checkpoint and the
    /// result, as the log took them.
    pub(crate) fn write_journal(&self, journal: &mut Journal) {
        let mut out = vec![JOURNAL_HEADER];
        self.put_header(&mut out);
        journal.append(&out);
        let mut start = 0;
        let starts = self.segments.first(self.segments.len());
        let mut segments = (1..).zip(starts.iter().skip(1)).peekable();
        for (tick, end) in (0..).zip(self.entries.tick_ends()) {
            if let Some((number, segment)) =
                segments.next_if(|(_, segment)| segment.tick == Tick::new(tick))
            {
                segment.journal_whole(journal, number, &mut out);
            }
            for at in start..end {
                self.journal_entry(journal, at, &mut out);
            }
            journal.append(&[JOURNAL_SEALED]);
            start = end;
        }
        if let Some((number, segment)) = segments.next() {
            segment.journal_whole(journal, number, &mut out);
        }
        for at in start..self.entries.len() {
            self.journal_entry(journal, at, &mut out);
        }
        if let Some(result) = &self.result {
            out.clear();
            write_result(&mut out, result);
            journal.append(&out);
        }
    }

    /// Sets in `journal`, which holds every record the log took, as the log was rebuilt from it,
    /// where each chain whose head its player's current delegation signed stands durably: where
    /// the log holds it, as the journal was read back from the disk.
    pub(crate) fn resume_durable(&self, journal: &mut Journal) {
        for (slot, held) in (0..).map(PlayerSlot::new).zip(self.control.iter()) {
            let Control::Player { delegation, chain } = held.control else {
                continue;
            };
            // A player who renewed their delegation since their last input has no head of theirs.
            if held.head_signer != Some(delegation) {
                continue;
            }
            let head = chain.next_seq().checked_sub(1).map(|seq| DurableHead {
                delegation: *self.entries.delegation(delegation).id(),
                seq,
                head: chain.head(),
            });
            journal.durable_mut().set(slot, head);
        }
    }

    /// Writes the entry `at` into `journal`, with `out` as scratch.
    fn journal_entry(&self, journal: &mut Journal, at: u32, out: &mut Vec<u8>) {
        out.clear();
        match self.entries.entry(at) {
            Entry::Packet(packet) => {
                out.push(JOURNAL_PACKET);
                self.put_packet(out, packet);
            }
            Entry::Server(server) => {
                out.push(JOURNAL_SERVER);
                self.put_server(out, server);
            }
        }
        journal.append(out);
    }
}
