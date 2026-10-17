//! The log's journal: the record of each thing the log takes, as it takes it, and the log
//! rebuilt from them.

use campfire_common::{PlayerSlot, Tick, Version};

use crate::control::Control;
use crate::journal::Journal;
use crate::journal::journal_frames::JournalFrames;
use crate::journal::record_sink::RecordSink;
use crate::session_log::applied::Applied;
use crate::session_log::durable_head::DurableHead;
use crate::session_log::error::{CheckpointError, JournalReplayError, LogError};
use crate::session_log::log_entries::Entry;
use crate::session_log::segments::{LoggedCheckpoint, SegmentStart};
use crate::session_log::{LoggedResult, SessionLog, file, put, take};

/// The bytes that start each kind of journal record.
const JOURNAL_HEADER: u8 = 0;
const JOURNAL_PACKET: u8 = 1;
const JOURNAL_SERVER: u8 = 2;
const JOURNAL_SEALED: u8 = 3;
const JOURNAL_CHECKPOINT_DONE: u8 = 4;
const JOURNAL_RESULT: u8 = 5;
const JOURNAL_CHECKPOINT_BEGUN: u8 = 6;
const JOURNAL_LOADED: u8 = 7;

impl SegmentStart {
    /// Writes the begin of the checkpoint that starts segment `segment`, and its record once it
    /// has one, into `journal`.
    fn journal_whole(&self, journal: &mut Journal, segment: u32) {
        self.journal_begun(journal, segment);
        if let Some(logged) = &self.checkpoint {
            logged.journal(journal);
        }
    }

    /// Writes the begin of the checkpoint that starts segment `segment` into `journal`.
    pub(super) fn journal_begun(&self, journal: &mut Journal, segment: u32) {
        journal.append(|out| {
            out.push(JOURNAL_CHECKPOINT_BEGUN);
            put(out, &segment);
            put(out, &self.tick);
        });
    }
}

impl LoggedCheckpoint {
    /// Writes the record into `journal`, as the checkpoint is done.
    pub(super) fn journal(&self, journal: &mut Journal) {
        journal.append(|out| {
            out.push(JOURNAL_CHECKPOINT_DONE);
            self.record.encode(out);
            put(out, &self.signature);
        });
    }
}

impl LoggedResult {
    /// Writes the result into `journal`, as the session ends.
    pub(super) fn journal(&self, journal: &mut Journal) {
        journal.append(|out| {
            out.push(JOURNAL_RESULT);
            put(out, self);
        });
    }
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
            JOURNAL_PACKET => self.record_packet(&mut rest, inputs, applied)?,
            JOURNAL_SERVER => self.record_server_input(&mut rest)?,
            JOURNAL_SEALED => drop(self.seal_tick()),
            JOURNAL_CHECKPOINT_BEGUN => {
                let segment: u32 = take(&mut rest)?;
                let tick: Tick = take(&mut rest)?;
                let begun = self
                    .begin_checkpoint()
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
                self.record_done(&mut rest, segment)?;
            }
            JOURNAL_LOADED => {
                let segment: u32 = take(&mut rest)?;
                self.load(segment)
                    .map_err(|error| LogError::Load { segment, error })?;
            }
            JOURNAL_RESULT => {
                let LoggedResult { result, signature } = take(&mut rest)?;
                self.record_result(result, &signature)
                    .map_err(LogError::Result)?;
            }
            _ => return Err(LogError::UnknownEntry),
        }
        if !rest.is_empty() {
            return Err(LogError::Trailing);
        }
        Ok(())
    }

    /// Keeps `sink` as its journal, a new one that holds the journal's tag alone, and writes into
    /// it the records of what the log holds: the header, then each entry, each sealed tick, each
    /// checkpoint and the result, as the log took them; from then on each record the log takes
    /// goes to it as it takes it.
    pub fn keep_journal(&mut self, sink: Box<dyn RecordSink>) {
        assert!(self.journal.is_none(), "a log keeps one journal");
        debug_assert!(
            self.to_replay.is_empty(),
            "a log that replays keeps no journal"
        );
        let tag = u64::try_from(JournalFrames::TAG.len()).expect("the tag's length fits u64");
        let mut journal = Journal::new(sink, tag);
        journal.append(|out| {
            out.push(JOURNAL_HEADER);
            self.put_header(out);
        });
        let mut start = 0;
        let starts = self.segments.first(self.segments.len());
        let mut segments = (1..).zip(starts.iter().skip(1)).peekable();
        for (tick, end) in (0..).zip(self.entries.tick_ends()) {
            if let Some((number, segment)) =
                segments.next_if(|(_, segment)| segment.tick == Tick::new(tick))
            {
                segment.journal_whole(&mut journal, number);
            }
            for at in start..end {
                self.journal_entry(&mut journal, at);
            }
            journal.append(|out| out.push(JOURNAL_SEALED));
            start = end;
        }
        if let Some((number, segment)) = segments.next() {
            segment.journal_whole(&mut journal, number);
        }
        for at in start..self.entries.len() {
            self.journal_entry(&mut journal, at);
        }
        if let Some(result) = &self.result {
            result.journal(&mut journal);
        }
        self.journal = Some(journal);
    }

    /// Keeps `sink` as its journal, of `len` bytes, which holds every record the log took, as the
    /// log was rebuilt from it: from then on each record the log takes goes to it as it takes it.
    /// Every chain whose head its player's current delegation signed stands durably where the log
    /// holds it, as the journal was read back from the disk.
    pub fn resume_journal(&mut self, sink: Box<dyn RecordSink>, len: u64) {
        assert!(self.journal.is_none(), "a log keeps one journal");
        self.journal = Some(Journal::new(sink, len));
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
            self.durable.set(slot, head);
        }
    }

    /// Closes its journal, the sink's drop writing what the sink holds before it ends, as a
    /// session that ends does before its log is published: the log keeps no journal after.
    pub fn close_journal(&mut self) {
        self.journal = None;
    }

    /// Notes where each player's chain stands once the first `durable` records the log wrote
    /// into its journal's sink are synced, as the sink's writer counts them.
    pub fn advance_durable(&mut self, durable: u64) {
        debug_assert!(
            self.journal
                .as_ref()
                .is_some_and(|journal| durable <= journal.written()),
            "durable records are records the log wrote into its journal"
        );
        self.durable.advance(durable);
    }

    /// Where `slot`'s player's chain stands in the records the journal synced, as of the last
    /// `advance_durable`; none before any, and since a player took the slot, renewed their
    /// delegation, or left it.
    pub fn durable_head(&self, slot: PlayerSlot) -> Option<DurableHead> {
        self.durable.get(slot)
    }

    /// Writes the entry the log took last into its journal, when it keeps one.
    pub(super) fn journal_last_entry(&mut self) {
        if let Some(mut journal) = self.journal.take() {
            self.journal_entry(&mut journal, self.entries.len() - 1);
            self.journal = Some(journal);
        }
    }

    /// Writes the entry `at` into `journal`.
    fn journal_entry(&self, journal: &mut Journal, at: u32) {
        journal.append(|out| match self.entries.entry(at) {
            Entry::Packet(packet) => {
                out.push(JOURNAL_PACKET);
                self.put_packet(out, packet);
            }
            Entry::Server(server) => {
                out.push(JOURNAL_SERVER);
                self.put_server(out, server);
            }
        });
    }
}

/// Writes into `journal` that the next tick was sealed.
pub(super) fn sealed(journal: &mut Journal) {
    journal.append(|out| out.push(JOURNAL_SEALED));
}

/// Writes into `journal` that the log went back to the save of segment `segment`.
pub(super) fn loaded(journal: &mut Journal, segment: u32) {
    journal.append(|out| {
        out.push(JOURNAL_LOADED);
        put(out, &segment);
    });
}
