use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use super::*;

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
    while !writer.shared.lock().bytes.is_empty() {
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
    assert_eq!(writer.shared.lock().bytes, b"cdgh");
    // "ab" is written, the worker takes "cdgh", and the late caller puts "ef".
    go.send(()).unwrap();
    waiting.join().unwrap();
    go.send(()).unwrap();
    go.send(()).unwrap();
    // Closed, the writer writes what it holds, then ends, with no failure.
    drop(sender);
    assert!(writer.close().is_none());
    assert_eq!(*written.lock().unwrap(), b"abcdghef");
}

#[test]
fn a_failed_write_stops_the_stream_keeps_its_failure_and_drops_what_follows() {
    let (writer, go, written) = gated(4);
    let sender = writer.sender();
    sender.put(|out| out.extend_from_slice(b"!x"));
    go.send(()).unwrap();
    while !writer.shared.lock().stopped {
        thread::yield_now();
    }
    // Pieces after the failure are dropped, even past the bound, with no wait.
    for _ in 0..3 {
        sender.put(|out| out.extend_from_slice(b"yyyy"));
    }
    // Closed, the writer gives the failure that stopped it.
    let failure = writer.close().unwrap();
    assert_eq!(failure.to_string(), "refused");
    assert!(written.lock().unwrap().is_empty());
    // A sender that outlives its writer puts nothing, and does not wait.
    sender.put(|out| out.extend_from_slice(b"zzzzzz"));
}
