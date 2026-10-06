use std::fs;
use std::io;
use std::sync::mpsc;
use std::thread;

use super::*;
use crate::durable_file::error::DurableError;
use crate::durable_file::tests::ScratchDir;

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

/// A file whose each sync waits for the test's word.
#[derive(Debug)]
struct GatedFile(mpsc::Receiver<()>);

impl AppendFile for GatedFile {
    fn append(&mut self, _: &[u8]) -> io::Result<()> {
        Ok(())
    }

    fn sync(&mut self) -> io::Result<()> {
        self.0.recv().map_err(io::Error::other)
    }
}

#[test]
fn a_writer_whose_file_fails_reports_the_failure_once_and_keeps_nothing_after() {
    for (append, sync) in [(true, false), (false, true)] {
        let writer = AppendWriter::start("failing", FailingFile { append, sync });
        let watch = writer.watch();
        writer.append(|out| out.extend_from_slice(b"lost"));
        while !watch.settled() {
            thread::yield_now();
        }
        // Stopped, it takes no record more.
        writer.append(|_| unreachable!("a stopped writer takes no record"));
        drop(writer);
        let failure = watch.take_failure();
        let step = match &failure {
            Some(AppendError::Write(error)) => (true, false, error.to_string()),
            Some(AppendError::Sync(error)) => (false, true, error.to_string()),
            None => panic!("the failure is reported"),
        };
        let message = if append {
            "append failed"
        } else {
            "sync failed"
        };
        assert_eq!(step, (append, sync, message.to_owned()));
        assert!(watch.take_failure().is_none());
        assert_eq!(watch.durable(), 0);
        assert!(watch.settled());
    }
}

#[test]
fn a_writer_is_settled_once_its_records_are_synced() {
    let (sync, gate) = mpsc::channel();
    let writer = AppendWriter::start("gated", GatedFile(gate));
    let watch = writer.watch();
    assert!(watch.settled());
    writer.append(|out| out.extend_from_slice(b"held"));
    assert!(!watch.settled());
    sync.send(()).unwrap();
    while watch.durable() < 1 {
        thread::yield_now();
    }
    assert!(watch.settled());
    drop(sync);
}

#[test]
fn a_new_file_holds_its_head_then_its_records_and_a_reopen_cuts_a_torn_tail() {
    let dir = ScratchDir::new("append");
    let path = dir.0.join("journal");
    let writer = AppendWriter::create("journal", &path, b"head/").unwrap();
    let watch = writer.watch();
    for record in [&b"first/"[..], b"", b"third/"] {
        writer.append(|out| out.extend_from_slice(record));
    }
    // Dropped, the writer writes and syncs what it holds.
    drop(writer);
    assert_eq!(watch.durable(), 3);
    assert!(watch.take_failure().is_none());
    assert_eq!(fs::read(&path).unwrap(), b"head/first/third/");

    // A crash tore the last record: reopened at the end of the whole ones, the file loses the
    // tear and goes on after them.
    fs::write(&path, b"head/first/thi").unwrap();
    let writer = AppendWriter::reopen("journal", &path, 11).unwrap();
    writer.append(|out| out.extend_from_slice(b"after/"));
    drop(writer);
    assert_eq!(fs::read(&path).unwrap(), b"head/first/after/");

    // A file that is not there does not reopen; one in a directory that is not there is not
    // made.
    assert!(matches!(
        AppendWriter::reopen("journal", &dir.0.join("none"), 0),
        Err(AppendOpenError::Open(_))
    ));
    assert!(matches!(
        AppendWriter::create("journal", &dir.0.join("none").join("journal"), b""),
        Err(AppendOpenError::Create(DurableError::Create(_)))
    ));
}
