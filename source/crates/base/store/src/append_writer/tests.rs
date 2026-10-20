use std::fs;
use std::io;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc;
use std::thread;

use tempfile::TempDir;

use super::*;
use crate::durable_file::error::DurableError;

/// A file whose appends or syncs fail, as its flags say.
#[derive(Debug)]
struct FailingFile {
    append: bool,
    sync: bool,
}

impl AppendFile for FailingFile {
    fn append(&mut self, _: &[u8]) -> io::Result<()> {
        if self.append {
            return Err(io::Error::other("append failed"));
        }
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        if self.sync {
            return Err(io::Error::other("sync failed"));
        }
        Ok(())
    }
}

/// A file whose each sync tells `entered` it began, after the writer started its clock, unless the
/// test stopped listening, then waits for the test's word on `gate`.
#[derive(Debug)]
struct GatedFile {
    gate: mpsc::Receiver<()>,
    entered: mpsc::Sender<()>,
}

impl AppendFile for GatedFile {
    fn append(&mut self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.entered.send(()).unwrap_or(());
        self.gate.recv().map_err(io::Error::other)
    }
}

/// Sends on its channel when dropped, so a test sees a drop return.
#[derive(Debug)]
struct SendOnDrop(mpsc::Sender<()>);

impl Drop for SendOnDrop {
    fn drop(&mut self) {
        self.0.send(()).unwrap_or(());
    }
}

#[test]
fn a_writer_whose_file_fails_reports_the_failure_once_and_keeps_nothing_after() {
    for (append, sync) in [(true, false), (false, true)] {
        let writer =
            AppendWriter::start(ThreadName::fixture("failing"), FailingFile { append, sync });
        let watch = writer.watch();
        writer.append(1, |out| out.extend_from_slice(b"lost"));
        while !watch.is_settled() {
            thread::yield_now();
        }
        // Stopped, it takes no record more. The failure is taken once, through the watch after
        // a drop, or from the close.
        writer.append(1, |_| unreachable!("a stopped writer takes no record"));
        let failure = if append {
            drop(writer);
            watch.take_failure()
        } else {
            writer.close()
        };
        let step = match &failure {
            Some(AppendError::Write(error)) => (true, false, error.to_string()),
            Some(AppendError::Sync(error)) => (false, true, error.to_string()),
            other => panic!("the file's failure is reported: {other:?}"),
        };
        let message = if append {
            "append failed"
        } else {
            "sync failed"
        };
        assert_eq!(step, (append, sync, message.to_owned()));
        assert!(watch.take_failure().is_none());
        assert_eq!(watch.durable(), 0);
        assert!(watch.is_settled());
    }
}

#[test]
fn a_writer_is_settled_once_its_records_are_synced_and_a_slow_sync_is_seen() {
    // Syncs past 200 ms are slow, for a test that holds one 300 ms, not a second; one answered
    // at once takes microseconds, far below.
    let slow_after = Duration::from_millis(200);
    let (sync, gate) = mpsc::channel();
    let (entered, in_sync) = mpsc::channel();
    let bounds = AppendBounds {
        slow_after,
        ..AppendWriter::BOUNDS
    };
    let writer = AppendWriter::start_bounded(
        ThreadName::fixture("gated"),
        GatedFile { gate, entered },
        bounds,
    );
    let watch = writer.watch();
    assert!(watch.is_settled());
    writer.append(1, |out| out.extend_from_slice(b"held"));
    assert!(!watch.is_settled());
    in_sync.recv().unwrap();
    sync.send(()).unwrap();
    while watch.durable() < 1 {
        thread::yield_now();
    }
    assert!(watch.is_settled());
    assert_eq!(watch.take_slow_sync(), None);

    // Held 300 ms from when the sync began, after the writer's clock started: slow, by at least
    // that, and taken once.
    writer.append(1, |out| out.extend_from_slice(b"slow"));
    in_sync.recv().unwrap();
    let held = Duration::from_millis(300);
    thread::sleep(held);
    sync.send(()).unwrap();
    while watch.durable() < 2 {
        thread::yield_now();
    }
    let slow = watch.take_slow_sync().expect("the held sync is slow");
    assert!(slow.took >= held, "{slow:?}");
    assert_eq!(watch.take_slow_sync(), None);
    drop(sync);
}

#[test]
fn a_writer_fails_once_its_sync_hangs_or_its_records_wait_past_the_bound() {
    // Hung past 100 ms, for a test that holds the sync 150 ms; the bound of the records that
    // wait is 8 bytes, so only the second record of 5 bytes passes it.
    let bounds = AppendBounds {
        hung_after: Duration::from_millis(100),
        max_behind: 8,
        ..AppendWriter::BOUNDS
    };
    let held = Duration::from_millis(150);
    for hung in [true, false] {
        let (release, gate) = mpsc::channel();
        let (entered, in_sync) = mpsc::channel();
        let writer = AppendWriter::start_bounded(
            ThreadName::fixture("held"),
            GatedFile { gate, entered },
            bounds,
        );
        let watch = writer.watch();
        writer.append(1, |out| out.extend_from_slice(b"held"));
        in_sync.recv().unwrap();
        if hung {
            // Asked before the bound, the sync that runs is no failure yet.
            assert!(watch.take_failure().is_none());
            thread::sleep(held);
        } else {
            writer.append(1, |out| out.extend_from_slice(b"five!"));
            assert!(watch.take_failure().is_none());
            writer.append(1, |out| out.extend_from_slice(b"five!"));
        }
        let failure = watch.take_failure();
        match (hung, &failure) {
            (true, Some(AppendError::Hung { running })) => assert!(*running >= held, "{running:?}"),
            (false, Some(AppendError::Behind { bytes: 10 })) => {}
            _ => panic!("{hung}: {failure:?}"),
        }
        // Stopped, it takes no record more, and the sync that returns late fails it no more.
        assert!(watch.is_settled());
        writer.append(1, |_| unreachable!("a stopped writer takes no record"));
        release.send(()).unwrap();
        assert!(writer.close().is_none());
        assert_eq!(watch.durable(), 1);
    }
}

#[test]
fn a_writer_dropped_in_a_panic_returns_while_its_sync_does_not() {
    // The worker's sync waits for a word that comes only once the test saw the drop return.
    let (release, gate) = mpsc::channel();
    let (returned, dropped) = mpsc::channel();
    let holder = Worker::start(ThreadName::fixture("holder"), move || {
        let _returned = SendOnDrop(returned);
        let (entered, in_sync) = mpsc::channel();
        let writer = AppendWriter::start(ThreadName::fixture("held"), GatedFile { gate, entered });
        writer.append(1, |out| out.extend_from_slice(b"held"));
        in_sync.recv().unwrap();
        // A record that panics as it is written poisons the lock the drop takes.
        writer.append(1, |_| panic!("the holder panics"));
    });
    dropped
        .recv_timeout(Duration::from_secs(10))
        .expect("the drop returns");
    // The holder's panic is its own, and no other passed on, which would have aborted.
    let payload = panic::catch_unwind(AssertUnwindSafe(|| drop(holder))).unwrap_err();
    assert_eq!(payload.downcast_ref::<&str>(), Some(&"the holder panics"));
    drop(release);
}

#[test]
fn a_new_file_holds_its_head_then_its_records_and_a_reopen_cuts_a_torn_tail() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("journal");
    let writer = AppendWriter::create(ThreadName::fixture("journal"), &path, b"head/").unwrap();
    let watch = writer.watch();
    for record in [&b"first/"[..], b"", b"third/"] {
        writer.append(1, |out| out.extend_from_slice(record));
    }
    // Two records in one append, as a journal hands a frame's records, count as two.
    writer.append(2, |out| out.extend_from_slice(b"fourth/fifth/"));
    // Dropped, the writer writes and syncs what it holds.
    drop(writer);
    assert_eq!(watch.durable(), 5);
    assert!(watch.take_failure().is_none());
    assert_eq!(fs::read(&path).unwrap(), b"head/first/third/fourth/fifth/");

    // A crash tore the last record: reopened at the end of the whole ones, the file loses the
    // tear and goes on after them.
    fs::write(&path, b"head/first/thi").unwrap();
    let writer = AppendWriter::reopen(ThreadName::fixture("journal"), &path, 11).unwrap();
    writer.append(1, |out| out.extend_from_slice(b"after/"));
    drop(writer);
    assert_eq!(fs::read(&path).unwrap(), b"head/first/after/");

    // A file that is not there does not reopen; one in a directory that is not there is not
    // made.
    assert!(matches!(
        AppendWriter::reopen(ThreadName::fixture("journal"), &dir.path().join("none"), 0),
        Err(PathError {
            error: AppendOpenError::Open(_),
            ..
        })
    ));
    assert!(matches!(
        AppendWriter::create(
            ThreadName::fixture("journal"),
            &dir.path().join("none").join("journal"),
            b""
        ),
        Err(PathError {
            error: AppendOpenError::Create(DurableError::Create(_)),
            ..
        })
    ));
}
