use crate::journal::record_sink::RecordSink;

pub(crate) mod error;
pub(crate) mod journal_frames;
pub(crate) mod record_sink;

/// A session log's journal: the sink its records go to, framed, how many it wrote there, and
/// how many bytes the journal's file holds.
#[derive(Debug)]
pub(crate) struct Journal {
    sink: Box<dyn RecordSink>,
    /// The records written into the sink since the log was given it.
    written: u64,
    /// The bytes of the file: the tag and every frame, those before the log was given the sink
    /// among them.
    len: u64,
}

impl Journal {
    /// The journal of `sink`, whose file holds `len` bytes so far.
    pub(crate) const fn new(sink: Box<dyn RecordSink>, len: u64) -> Self {
        Self {
            sink,
            written: 0,
            len,
        }
    }

    /// Frames the record `write` puts in the buffer it is given, and hands it to the sink.
    pub(crate) fn append(&mut self, mut write: impl FnMut(&mut Vec<u8>)) {
        let mut framed = 0;
        self.sink.append(&mut |out| {
            let start = journal_frames::open(out);
            write(out);
            journal_frames::seal(out, start);
            framed = out.len() - start;
        });
        self.written += 1;
        self.len += u64::try_from(framed).expect("a frame's length fits u64");
    }

    /// How many records the log wrote into the sink.
    pub(crate) const fn written(&self) -> u64 {
        self.written
    }

    /// How many bytes the journal's file holds.
    pub(crate) const fn len(&self) -> u64 {
        self.len
    }
}

#[cfg(test)]
pub(crate) mod tests;
