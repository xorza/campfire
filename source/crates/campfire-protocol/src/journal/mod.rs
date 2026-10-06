use crate::journal::journal_frames::JournalFrames;
use crate::journal::record_sink::RecordSink;

pub(crate) mod error;
pub(crate) mod journal_frames;
pub(crate) mod record_sink;

/// A session log's journal: the sink its records go to, framed, and how many it wrote there.
#[derive(Debug)]
pub(crate) struct Journal {
    sink: Box<dyn RecordSink + Send + Sync>,
    /// The records written into the sink since the log was given it.
    written: u64,
}

impl Journal {
    pub(crate) fn new(sink: Box<dyn RecordSink + Send + Sync>) -> Journal {
        Journal { sink, written: 0 }
    }

    /// Frames the record `write` puts in the buffer it is given, and hands it to the sink.
    pub(crate) fn append(&mut self, mut write: impl FnMut(&mut Vec<u8>)) {
        self.sink.append(&mut |out| {
            let start = JournalFrames::open(out);
            write(out);
            JournalFrames::seal(out, start);
        });
        self.written += 1;
    }

    /// How many records the log wrote into the sink.
    pub(crate) const fn written(&self) -> u64 {
        self.written
    }
}

#[cfg(test)]
pub(crate) mod tests;
