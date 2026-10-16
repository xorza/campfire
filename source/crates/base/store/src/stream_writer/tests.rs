use std::panic::{self, AssertUnwindSafe};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use super::*;
use crate::scratch::Scratch;

/// A file that holds what is written to it, each write after the test's word, and fails each
/// write of a piece that starts with `!`.
#[derive(Debug)]
struct Gated {
    gate: Receiver<()>,
    written: Arc<Mutex<Vec<u8>>>,
}

impl Write for Gated {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.gate.recv().unwrap();
        if bytes.first() == Some(&b'!') {
            return Err(io::Error::other("refused"));
        }
        self.written.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// A writer of `bound` bytes over a gated file, the word to each write, and what the file holds.
fn gated(bound: usize) -> (StreamWriter, mpsc::Sender<()>, Arc<Mutex<Vec<u8>>>) {
    let (go, gate) = mpsc::channel();
    let written = Arc::new(Mutex::new(Vec::new()));
    let file = Gated {
        gate,
        written: Arc::clone(&written),
    };
    (StreamWriter::start("stream", file, bound), go, written)
}

/// Waits until the worker took every piece put so far.
fn taken(writer: &StreamWriter) {
    while !writer.shared.pending.lock().bytes.is_empty() {
        thread::yield_now();
    }
}

#[test]
fn pieces_go_out_in_order_and_a_full_buffer_makes_a_caller_wait() {
    // A buffer of 4 bytes. "ab" is taken at once and its write waits for its word; "cd" and "gh"
    // then fill the buffer, so "ef" waits until the worker takes them.
    let (writer, go, written) = gated(4);
    let sender = writer.sender();
    sender.put(|out| out.extend_from_slice(b"ab"));
    taken(&writer);
    sender.put(|out| out.extend_from_slice(b"cd"));
    sender.put(|out| out.extend_from_slice(b"gh"));
    let late = sender.clone();
    let waiting = thread::spawn(move || late.put(|out| out.extend_from_slice(b"ef")));
    // Given the time to run, the late caller has put nothing: the buffer is full.
    thread::sleep(Duration::from_millis(20));
    assert_eq!(writer.shared.pending.lock().bytes, b"cdgh");
    // "ab" is written, the worker takes "cdgh", and the late caller puts "ef".
    go.send(()).unwrap();
    waiting.join().unwrap();
    // The writer closes while the write of "cdgh" waits for its word: a piece put then is
    // dropped at once, so a caller that goes on putting cannot keep the worker running.
    let shared = Arc::clone(&writer.shared);
    let closing = thread::spawn(move || writer.close());
    while !shared.pending.lock().closing {
        thread::yield_now();
    }
    sender.put(|out| out.extend_from_slice(b"zz"));
    assert_eq!(shared.pending.lock().bytes, b"ef");
    // The worker writes "cdgh" and "ef", what the buffer held, then ends, with no failure.
    go.send(()).unwrap();
    go.send(()).unwrap();
    assert!(closing.join().unwrap().is_none());
    assert_eq!(*written.lock().unwrap(), b"abcdghef");
}

#[test]
fn a_failed_write_stops_the_stream_keeps_its_failure_and_drops_what_follows() {
    let (writer, go, written) = gated(4);
    let sender = writer.sender();
    sender.put(|out| out.extend_from_slice(b"!x"));
    go.send(()).unwrap();
    while !writer.shared.pending.lock().stopped {
        thread::yield_now();
    }
    // Pieces after the failure are dropped, even past the bound, with no wait.
    for _ in 0..3 {
        sender.put(|out| out.extend_from_slice(b"yyyy"));
    }
    // Closed, the writer gives the failure that stopped it.
    let Some(StreamError::Write(failure)) = writer.close() else {
        panic!("the write failed");
    };
    assert_eq!(failure.to_string(), "refused");
    assert!(written.lock().unwrap().is_empty());
    // A sender that outlives its writer puts nothing, and does not wait.
    sender.put(|out| out.extend_from_slice(b"zzzzzz"));
}

/// A file that takes each write and refuses each flush.
#[derive(Debug)]
struct Unflushed;

impl Write for Unflushed {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("unflushed"))
    }
}

#[test]
fn a_failed_flush_is_taken_once_while_the_stream_runs() {
    let writer = StreamWriter::start("stream", Unflushed, 4);
    assert!(writer.take_failure().is_none());
    writer.sender().put(|out| out.extend_from_slice(b"ab"));
    let failure = loop {
        if let Some(failure) = writer.take_failure() {
            break failure;
        }
        thread::yield_now();
    };
    assert!(matches!(&failure, StreamError::Flush(error) if error.to_string() == "unflushed"));
    // Taken, the failure is gone: the close gives none.
    assert!(writer.take_failure().is_none());
    assert!(writer.close().is_none());
}

#[test]
fn a_writer_dropped_as_a_panic_unwinds_closes_over_a_piece_that_panicked() {
    // The piece's panic poisons the lock; a drop that locked it as the worker does would panic
    // again as the next panic unwinds, which aborts.
    let (writer, _go, _written) = gated(4);
    let sender = writer.sender();
    let put = panic::catch_unwind(AssertUnwindSafe(|| sender.put(|_| panic!("torn"))));
    assert!(put.is_err());
    let unwound = panic::catch_unwind(AssertUnwindSafe(move || {
        let _held = writer;
        panic::panic_any(9_u32);
    }));
    assert_eq!(unwound.unwrap_err().downcast_ref::<u32>(), Some(&9));
}

#[test]
fn a_created_stream_is_its_owners_only_and_empties_a_file_there() {
    let scratch = Scratch::new();
    let fresh = scratch.path("fresh");
    drop(StreamWriter::create("stream", &fresh, 16).unwrap());
    assert_eq!(OwnerOnly::exposure_at(&fresh).unwrap(), None);
    assert_eq!(scratch.read_text(Path::new("fresh")), "");
    // A file there is emptied before the stream's first piece.
    let kept = Path::new("kept");
    scratch.write(kept, "old text");
    let writer = StreamWriter::create("stream", &scratch.path("kept"), 16).unwrap();
    writer.sender().put(|out| out.extend_from_slice(b"new"));
    assert!(writer.close().is_none());
    assert_eq!(scratch.read_text(kept), "new");
}
