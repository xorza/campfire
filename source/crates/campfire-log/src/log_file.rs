use std::io::{self, Write};

use campfire_store::{StreamSender, StreamWriter};
use tracing::error;
use tracing_subscriber::fmt::MakeWriter;

use crate::error_report::ErrorReport;

/// The JSON log file while the binary runs, which a worker writes, so the thread that logs never
/// waits for the disk. The binary holds it from `Logging::start` to the end of `main`: dropped,
/// it writes the lines its worker still holds, and logs a write that failed. A crash that
/// aborts loses the lines its worker held.
#[must_use = "dropping it ends the file's worker, and the lines after that are lost"]
#[derive(Debug, Default)]
pub struct LogFile(Option<StreamWriter>);

/// The file layer's writer: each event's line goes into the worker's buffer.
#[derive(Debug, Clone)]
pub(crate) struct LogLines(pub(crate) StreamSender);

/// One event's line, as the layer writes it.
#[derive(Debug)]
pub(crate) struct LineWriter<'a>(&'a StreamSender);

impl LogFile {
    pub(crate) const fn of(writer: StreamWriter) -> LogFile {
        LogFile(Some(writer))
    }
}

impl Drop for LogFile {
    fn drop(&mut self) {
        let Some(writer) = self.0.take() else {
            return;
        };
        if let Some(failure) = writer.close() {
            error!(error = %ErrorReport::of(&failure), "the log file stopped at a write that failed");
        }
    }
}

impl<'a> MakeWriter<'a> for LogLines {
    type Writer = LineWriter<'a>;

    fn make_writer(&'a self) -> LineWriter<'a> {
        LineWriter(&self.0)
    }
}

impl Write for LineWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.put(|out| out.extend_from_slice(bytes));
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
